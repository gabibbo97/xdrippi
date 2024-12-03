use std::{os::fd::{AsRawFd, RawFd}, sync::Arc};

use crate::{utils, Umem, XDPRing};

/// An AF_XDP socket bound to an <ifindex,ifqueue> pair
pub struct XDPSocket<'a> {
    // metadata
    pub if_index: libc::c_uint,
    pub if_queue: libc::c_uint,

    // memory
    pub umem: Arc<Umem>,

    // socket
    pub fd: RawFd,

    // rings
    pub rx_ring: XDPRing<'a, libc::xdp_desc>,
    pub tx_ring: XDPRing<'a, libc::xdp_desc>,
    pub completion_ring: XDPRing<'a, u64>,
    pub fill_ring: XDPRing<'a, u64>,
}
impl<'a> XDPSocket<'a> {

    /// Create a new AF_XDP socket bound to the interface with index `interface_index` and its queue `queue_id`.
    /// Use the provided `umem`.
    /// `rings_size` indicates the size of all rings, if in doubt, upstream uses 2048.
    pub fn new(
        interface_index: libc::c_uint,
        queue_id: libc::c_uint,
        umem: Arc<Umem>,
        rings_size: usize,
    ) -> Result<Self, crate::Error> {
        // check rings size
        assert!(rings_size.is_power_of_two(), "rings_size must be a power of two");

        // create AF_XDP socket
        let fd = unsafe { libc::socket(libc::AF_XDP, libc::SOCK_RAW, 0) };
        if fd < 0 {
            return Err(crate::Error::SocketCreationFailure);
        }

        // register umem with socket
        utils::setsockopt(fd, libc::SOL_XDP, libc::XDP_UMEM_REG, &libc::xdp_umem_reg_v1 {
            addr: unsafe { umem.memory_ptr() } as usize as _,
            len: umem.memory_size() as _,
            chunk_size: umem.chunk_size() as _,
            headroom: 0,
        })?;

        // prepare rings
        utils::setsockopt(fd, libc::SOL_XDP, libc::XDP_RX_RING, &rings_size)?;
        utils::setsockopt(fd, libc::SOL_XDP, libc::XDP_TX_RING, &rings_size)?;
        utils::setsockopt(fd, libc::SOL_XDP, libc::XDP_UMEM_FILL_RING, &rings_size)?;
        utils::setsockopt(fd, libc::SOL_XDP, libc::XDP_UMEM_COMPLETION_RING, &rings_size)?;

        // get rings umem offsets
        let umem_offsets = utils::getsockopt::<libc::xdp_mmap_offsets_v1>(fd, libc::SOL_XDP, libc::XDP_MMAP_OFFSETS)?;

        // mmap rings
        let rx_ring = XDPRing::new(rings_size, fd, &umem_offsets.rx, libc::XDP_PGOFF_RX_RING)?;
        let tx_ring = XDPRing::new(rings_size, fd, &umem_offsets.tx, libc::XDP_PGOFF_TX_RING)?;
        let cp_ring = XDPRing::new(rings_size, fd, &umem_offsets.cr, libc::XDP_UMEM_PGOFF_COMPLETION_RING as _)?;
        let fl_ring = XDPRing::new(rings_size, fd, &umem_offsets.fr, libc::XDP_UMEM_PGOFF_FILL_RING as _)?;

        // bind socket
        let bind_address = libc::sockaddr_xdp {
            sxdp_family: libc::AF_XDP as _,
            sxdp_flags: libc::XDP_USE_NEED_WAKEUP,
            sxdp_ifindex: interface_index,
            sxdp_queue_id: queue_id,
            sxdp_shared_umem_fd: 0,
        };
        let bind_result = unsafe { libc::bind(fd, &bind_address as *const _ as *const _, std::mem::size_of::<libc::sockaddr_xdp>() as _) };
        if bind_result < 0 {
            return Err(crate::Error::SocketBindFailure { error: std::io::Error::last_os_error() });
        }

        // assemble result
        Ok(Self {
            if_index: interface_index,
            if_queue: queue_id,
            umem,
            fd,
            rx_ring,
            tx_ring,
            completion_ring: cp_ring,
            fill_ring: fl_ring,
        })
    }

    /// Gets the statistics associated with this AF_XDP socket
    pub fn get_statistics(&self) -> Result<libc::xdp_statistics_v1, crate::Error> {
        utils::getsockopt(self.fd, libc::SOL_XDP, libc::XDP_STATISTICS)
    }

    /// Gets the options associated with this AF_XDP socket
    pub fn get_options(&self) -> Result<libc::xdp_options, crate::Error> {
        utils::getsockopt(self.fd, libc::SOL_XDP, libc::XDP_OPTIONS)
    }

    /// Poll this socket for new packets
    /// 
    /// _You should not use this function unless in development, and leverage some sort of reactor instead_
    pub fn poll_for_reception(&self) -> Result<(), crate::Error> {
        let mut poll_fd = libc::pollfd {
            fd: self.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        unsafe { libc::poll(&mut poll_fd as *mut _ as *mut _, 1, -1) };
        if (poll_fd.revents & libc::POLLIN) == 0 {
            Err(crate::Error::PollFailure)
        } else {
            Ok(())
        }
    }

    /// Wake this socket up for transmission
    pub fn wake_for_transmission(&self) -> Result<(), crate::Error> {
        let ret = unsafe { libc::sendto(self.fd, std::ptr::null(), 0,  libc::MSG_DONTWAIT, std::ptr::null(), 0) };
        if ret < 0 {
            Err(crate::Error::SocketSendFailure { error: std::io::Error::last_os_error() })
        } else {
            Ok(())
        }
    }

    pub fn debug_print_status(&self) {
        println!("stats for AF_XDP sock {}", self.fd);
        let stats = self.get_statistics().unwrap();
        println!("  rx dropped (other reason)       = {}", stats.rx_dropped);
        println!("  rx dropped (invalid descriptor) = {}", stats.rx_invalid_descs);
        println!("  tx dropped (invalid descriptor) = {}", stats.tx_invalid_descs);
        fn debug_ring<D>(name: &str, ring: &XDPRing<D>) {
            print!("{name} ring (");
            print!("consumer idx = {:10}", ring.get_consumer_index());
            print!(", producer idx = {:10}", ring.get_producer_index());
            println!(")");
        }
        debug_ring("TX", &self.tx_ring);
        debug_ring("RX", &self.rx_ring);
        debug_ring("CP", &self.completion_ring);
        debug_ring("FL", &self.fill_ring);
    }
}
impl<'a> AsRawFd for XDPSocket<'a> {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}
impl<'a> Drop for XDPSocket<'a> {
    fn drop(&mut self) {
        // close socket
        unsafe { libc::close(self.fd) };
    }
}
