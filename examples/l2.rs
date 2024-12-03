use std::{collections::HashMap, sync::Arc};
use std::os::fd::AsRawFd;

use xdrippi::{utils::interface_name_to_index, BPFRedirectManager, Umem, UmemAllocator, XDPSocket};

fn setup_af_xdp_for(interface_name: &str) -> (BPFRedirectManager, XDPSocket<'_>, UmemAllocator) {
    let if_index = interface_name_to_index(interface_name).unwrap();
    let umem = Umem::new_2k(16384).unwrap();
    let umem = Arc::new(umem);
    let sock = XDPSocket::new(if_index, 0, umem.clone(), 4096).unwrap();
    let mut bpf_manager = BPFRedirectManager::attach(if_index);
    bpf_manager.add_redirect(0, sock.as_raw_fd());
    let umem_allocator = UmemAllocator::for_umem(umem.clone());
    (bpf_manager, sock, umem_allocator)
}

fn main() {
    const IF_NAMES: &'static [&str] = &[ "test1", "test2", "test3", "test4", "test5", "test6", "test7", "test8" ];

    // create sockets
    let mut socks = IF_NAMES.iter()
        .map(|name| setup_af_xdp_for(&name))
        .collect::<Vec<_>>();

    // allocate fill rings
    for (_, sock, allocator) in &mut socks {
        while let Some(chunk_index) = allocator.try_allocate() {
            if sock.fill_ring.can_produce() {
                sock.fill_ring.produce_umem_offset(sock.umem.chunk_start_offset_for_index(chunk_index));
            } else {
                allocator.release(chunk_index);
                break;
            }
        }
    }

    // prepare structure
    let mut switch_table: HashMap<[u8;6], usize> = HashMap::new();

    // receive
    let mut poll_fds = socks.iter()
            .map(|(_, sock, _)| libc::pollfd {
                fd: sock.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            })
            .collect::<Vec<_>>();

    loop {
        // poll
        println!("==> Polling");
        unsafe { libc::poll(poll_fds.as_mut_ptr(), poll_fds.len() as _, -1); }

        // receive traffic
        let mut traffic: Vec<(usize, Vec<u8>)> = Vec::new();
        for (i, _) in poll_fds.iter().enumerate().filter(|(_, fd)| fd.revents & libc::POLLIN != 0) {
            println!("Received on socket {i}");
            let (_, sock, allocator) = &mut socks[i];
            while sock.rx_ring.can_consume() {
                // process inbound packet
                let rx_descriptor = sock.rx_ring.get_nth_descriptor(sock.rx_ring.get_consumer_index() as _);
                let rx_slice = sock.rx_ring.get_nth_slice(sock.rx_ring.get_consumer_index() as _, &sock.umem);
                let eth_dst_addr: &[u8; 6] = &rx_slice[0..6].try_into().unwrap();
                let eth_src_addr: &[u8; 6] = &rx_slice[6..12].try_into().unwrap();

                // learn src addr
                if ! switch_table.contains_key(eth_src_addr) {
                    println!("Learned {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} => {i}", eth_src_addr[0], eth_src_addr[1], eth_src_addr[2], eth_src_addr[3], eth_src_addr[4], eth_src_addr[5]);
                    switch_table.insert(eth_src_addr.clone(), i);
                }

                // dispatch to other queues
                if let Some(out_sock_idx) = switch_table.get(eth_dst_addr) {
                    // send to port
                    println!("DMAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} goes to port {out_sock_idx}", eth_dst_addr[0], eth_dst_addr[1], eth_dst_addr[2], eth_dst_addr[3], eth_dst_addr[4], eth_dst_addr[5]);
                    traffic.push((*out_sock_idx, rx_slice.to_vec()));
                } else {
                    // flood
                    if eth_dst_addr == &[ 0xFF; 6 ] {
                        println!("DMAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} broadcast, flooding", eth_dst_addr[0], eth_dst_addr[1], eth_dst_addr[2], eth_dst_addr[3], eth_dst_addr[4], eth_dst_addr[5]);
                    } else {
                        println!("DMAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} unknown, flooding", eth_dst_addr[0], eth_dst_addr[1], eth_dst_addr[2], eth_dst_addr[3], eth_dst_addr[4], eth_dst_addr[5]);
                    }
                    for j in 0..poll_fds.len() {
                        if i == j { continue; }
                        traffic.push((j, rx_slice.to_vec()));
                    }
                }

                // refill allocator or fill ring
                if sock.fill_ring.can_produce() {
                    sock.fill_ring.produce_umem_offset(rx_descriptor.addr);
                } else {
                    allocator.release_offset(rx_descriptor.addr);
                }

                // advance index
                sock.rx_ring.advance_consumer_index();
            }
        }

        // send traffic
        println!("==> Sending");
        for (out_sock_idx, data) in traffic {
            let (_, sock, allocator) = &mut socks[out_sock_idx];
            if let Some(chunk_index) = allocator.try_allocate() {
                // copy traffic
                let tx_offset = sock.umem.chunk_start_offset_for_index(chunk_index);
                let tx_slice = sock.tx_ring.get_nth_slice_mut(sock.tx_ring.get_producer_index() as _, &sock.umem, Some(tx_offset), Some(data.len() as _));
                tx_slice.copy_from_slice(&data);

                // advance tx index
                sock.tx_ring.advance_producer_index();

                // send message
                sock.wake_for_transmission().unwrap();
            } else {
                eprintln!("Failed sending to socket {out_sock_idx}");
            }
        }

        // refill allocators from completion rings
        for (_, sock, allocator) in &mut socks {
            while sock.completion_ring.can_consume() {
                let offset = sock.completion_ring.get_nth_umem_offset(sock.completion_ring.get_consumer_index() as _);
                allocator.release_offset(offset);
                sock.completion_ring.advance_consumer_index();
            }
        }

        // refill fill rings from allocators
        for (_, sock, allocator) in &mut socks {
            while let Some(chunk_index) = allocator.try_allocate() {
                if sock.fill_ring.can_produce() {
                    sock.fill_ring.produce_umem_offset(sock.umem.chunk_start_offset_for_index(chunk_index));
                } else {
                    allocator.release(chunk_index);
                    break;
                }
            }
        }

        // print MAC table
        println!("==> MAC TABLE");
        for (dmac, idx) in switch_table.iter() {
            println!("  {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} => {idx}", dmac[0], dmac[1], dmac[2], dmac[3], dmac[4], dmac[5]);
        }
    }
}