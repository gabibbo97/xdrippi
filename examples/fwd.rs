use xdrippi::{utils::interface_name_to_index, BPFRedirectManager, DefaultAllocator, Umem, UmemAllocator, XDPSocket};

use std::{os::fd::AsRawFd, sync::Arc};

fn main() {
    tracing_subscriber::fmt::init();

    // socket 1
    let if1_index = interface_name_to_index("test1").unwrap();
    let umem1 = Umem::new_2k(16384).unwrap();
    let umem1 = Arc::new(umem1);
    let mut sock1 = XDPSocket::new(if1_index, 0, umem1.clone(), 4096).unwrap();
    let mut bpf1_manager = BPFRedirectManager::attach(if1_index);
    bpf1_manager.add_redirect(0, sock1.as_raw_fd());
    let umem1_allocator = DefaultAllocator::for_umem(sock1.umem.clone());

    // socket 2
    let if2_index = interface_name_to_index("test2").unwrap();
    let umem2 = Umem::new_2k(16384).unwrap();
    let umem2 = Arc::new(umem2);
    let mut sock2 = XDPSocket::new(if2_index, 0, umem2.clone(), 4096).unwrap();
    let mut bpf2_manager = BPFRedirectManager::attach(if2_index);
    bpf2_manager.add_redirect(0, sock2.as_raw_fd());
    let umem2_allocator = DefaultAllocator::for_umem(sock2.umem.clone());

    // allocate fill rings
    while let Some(chunk_index) = umem1_allocator.try_allocate() {
        if sock1.fill_ring.can_produce() {
            sock1.fill_ring.produce_umem_offset(umem1.chunk_start_offset_for_index(chunk_index));
        } else {
            umem1_allocator.release(chunk_index);
            break;
        }
    }
    while let Some(chunk_index) = umem2_allocator.try_allocate() {
        if sock2.fill_ring.can_produce() {
            sock2.fill_ring.produce_umem_offset(umem2.chunk_start_offset_for_index(chunk_index));
        } else {
            umem2_allocator.release(chunk_index);
            break;
        }
    }

    // receive
    let mut poll_fds = [
        libc::pollfd {
            fd: sock1.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: sock2.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
    ];
    loop {
        // poll
        unsafe { libc::poll(&mut poll_fds as *mut _ as *mut _, 2, -1) };

        // RX traffic
        for fd in [ 0, 1 ] {
            // skip if no event
            if (poll_fds[fd].revents & libc::POLLIN) == 0 {
                continue;
            }
            
            // get rx and tx sockets
            let (rx_sock, rx_allocator, tx_sock, tx_allocator) = match fd {
                0 => {
                    // println!("1 -> 2");
                    (&mut sock1, &umem1_allocator, &mut sock2, &umem2_allocator)
                },
                1 => {
                    // println!("2 -> 1");
                    (&mut sock2, &umem2_allocator, &mut sock1, &umem1_allocator)
                },
                _ => unreachable!(),
            };

            // process received
            while rx_sock.rx_ring.can_consume() {
                let rx_descriptor = rx_sock.rx_ring.get_nth_descriptor(rx_sock.rx_ring.get_consumer_index() as _);
                // dbg!(rx_descriptor.addr);
                // dbg!(rx_descriptor.len);

                // give back address to allocator ring
                if rx_sock.fill_ring.can_produce() {
                    rx_sock.fill_ring.produce_umem_offset(rx_descriptor.addr);
                } else {
                    rx_allocator.release_offset(rx_descriptor.addr);
                }

                // send to other socket
                if tx_sock.tx_ring.can_produce() {
                    if let Some(chunk_index) = tx_allocator.try_allocate() {
                        // grab memory from first socket
                        let rx_slice = rx_sock.rx_ring.get_nth_slice(rx_sock.rx_ring.get_consumer_index() as _, &rx_sock.umem);

                        // copy memory to second socket umem
                        let tx_offset = tx_sock.umem.chunk_start_offset_for_index(chunk_index);
                        let tx_slice = tx_sock.tx_ring.get_nth_slice_mut(tx_sock.tx_ring.get_producer_index() as _, &tx_sock.umem, Some(tx_offset), Some(rx_descriptor.len as _));
                        tx_slice.copy_from_slice(rx_slice);

                        // advance tx index
                        tx_sock.tx_ring.advance_producer_index();

                        // send message
                        tx_sock.wake_for_transmission().unwrap();
                    } else {
                        eprintln!("  could not allocate for TX");
                        break
                    }
                }

                // advance rx index
                rx_sock.rx_ring.advance_consumer_index();
            }
        }

        // refill allocators from completion rings
        while sock1.completion_ring.can_consume() {
            let offset = sock1.completion_ring.get_nth_umem_offset(sock1.completion_ring.get_consumer_index() as _);
            umem1_allocator.release_offset(offset);
            sock1.completion_ring.advance_consumer_index();
        }
        while sock2.completion_ring.can_consume() {
            let offset = sock2.completion_ring.get_nth_umem_offset(sock2.completion_ring.get_consumer_index() as _);
            umem2_allocator.release_offset(offset);
            sock2.completion_ring.advance_consumer_index();
        }

        // refill from allocators
        while let Some(chunk_index) = umem1_allocator.try_allocate() {
            if sock1.fill_ring.can_produce() {
                sock1.fill_ring.produce_umem_offset(sock1.umem.chunk_start_offset_for_index(chunk_index));
            } else {
                break;
            }
        }
        while let Some(chunk_index) = umem2_allocator.try_allocate() {
            if sock2.fill_ring.can_produce() {
                sock2.fill_ring.produce_umem_offset(sock2.umem.chunk_start_offset_for_index(chunk_index));
            } else {
                break;
            }
        }

    }
}