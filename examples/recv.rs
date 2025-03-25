use xdrippi::{utils::interface_name_to_index, BPFRedirectManager, DefaultAllocator, Umem, UmemAllocator, XDPSocket};

use std::{os::fd::AsRawFd, sync::Arc};

fn main() {
    tracing_subscriber::fmt::init();

    let if_index = interface_name_to_index("test1").unwrap();

    let umem = Umem::new_2k(512).unwrap();
    let umem = Arc::new(umem);
    let mut sock = XDPSocket::new(if_index, 0, umem.clone(), 512).unwrap();

    // bpf
    let mut bpf_manager = BPFRedirectManager::attach(if_index);
    bpf_manager.add_redirect(0, sock.as_raw_fd());

    // umem allocator
    let umem_allocator = DefaultAllocator::for_umem(sock.umem.clone());

    // fill the fill ring
    sock.debug_print_status();

    while let Some(chunk_index) = umem_allocator.try_allocate() {
        if sock.fill_ring.can_produce() {
            sock.fill_ring.produce_umem_offset(umem.chunk_start_offset_for_index(chunk_index));
        } else {
            umem_allocator.release(chunk_index);
            break;
        }
    }
    sock.debug_print_status();

    // receive
    loop {
        // receive packet
        sock.poll_for_reception().unwrap();

        // explore RX ring
        while sock.rx_ring.can_consume() {
            let rx_descriptor = sock.rx_ring.get_nth_descriptor(sock.rx_ring.get_consumer_index() as _);
            dbg!(rx_descriptor.addr);
            dbg!(rx_descriptor.len);

            // read packet contents
            {
                let rx_buffer = sock.rx_ring.get_nth_slice(sock.rx_ring.get_consumer_index() as _, &sock.umem);
                let rx_string = String::from_utf8_lossy(&rx_buffer);
                println!("Received: {rx_string:?}");
            }

            // give back to fill ring
            if sock.fill_ring.can_produce() {
                sock.fill_ring.produce_umem_offset(rx_descriptor.addr);
            } else {
                umem_allocator.release_offset(rx_descriptor.addr);
            }

            // advance indexes
            sock.rx_ring.advance_consumer_index();
        }

        sock.debug_print_status();
    }
}