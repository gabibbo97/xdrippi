use std::os::fd::AsRawFd;

use crate::Umem;

/// The reference to an XDP ring (tx,rx,completion,fill)
/// 
/// - Completion and fill rings have [`libc::xdp_desc`] as their `D` type parameter
/// - TX and RX rings have [`u64`] as their `D` type parameter
pub struct XDPRing<'a, D> {
    // metadata
    mmap_size: usize,
    num_elements: usize,

    // pointers
    consumer_index: &'a std::sync::atomic::AtomicU32,
    producer_index: &'a std::sync::atomic::AtomicU32,
    descriptors: &'a mut [D],
}
impl<'a, D> XDPRing<'a, D> {
    //
    // construction
    //

    /// Construct a ring of `num_elements` size for the socket given in `sock_fd`
    /// 
    /// - `sock_offsets` is one of the fields obtained in the [`libc::xdp_mmap_offsets_v1`] structure originated by a [`libc::XDP_MMAP_OFFSETS`] getsockopt call
    /// - `ring_offset` is the mmap offset associated with the type of ring, i.e. [`libc::XDP_PGOFF_RX_RING`], [`libc::XDP_PGOFF_TX_RING`], [`libc::XDP_UMEM_PGOFF_COMPLETION_RING`], [`libc::XDP_UMEM_PGOFF_FILL_RING`]
    pub fn new(num_elements: usize, sock_fd: impl AsRawFd, sock_offsets: &libc::xdp_ring_offset_v1, ring_offset: libc::off_t) -> Result<Self, crate::Error> {
        // mmap ring
        let mmap_size = sock_offsets.desc as usize + std::mem::size_of::<D>() * num_elements;
        let mmap_base = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                mmap_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED | libc::MAP_POPULATE,
                sock_fd.as_raw_fd(),
                ring_offset
            )
        };
        if mmap_base == libc::MAP_FAILED || mmap_base.is_null() {
            return Err(crate::Error::MemoryMapFailure);
        }

        // create self
        unsafe {
            Ok(
                Self {
                    mmap_size,
                    num_elements,
                    consumer_index: std::sync::atomic::AtomicU32::from_ptr(mmap_base.byte_add(sock_offsets.consumer as _).cast()),
                    producer_index: std::sync::atomic::AtomicU32::from_ptr(mmap_base.byte_add(sock_offsets.producer as _).cast()),
                    descriptors: std::slice::from_raw_parts_mut(mmap_base.byte_add(sock_offsets.desc as _) as *mut D, num_elements),
                }
            )
        }
    }

    //
    // access
    //

    /// The size of this ring
    pub const fn num_elements(&self) -> usize {
        self.num_elements
    }

    const fn num_elements_mask(&self) -> u32 {
        self.num_elements as u32 - 1
    }

    // consumer

    /// The next index from which the consumer should read
    pub fn get_consumer_index(&self) -> u32 {
        self.consumer_index.load(std::sync::atomic::Ordering::Relaxed) & self.num_elements_mask()
    }

    /// Advance the consumer index by one
    pub fn advance_consumer_index(&mut self) {
        self.consumer_index.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    // producer

    /// The next index to which the producer should produce
    pub fn get_producer_index(&self) -> u32 {
        self.producer_index.load(std::sync::atomic::Ordering::Relaxed) & self.num_elements_mask()
    }

    /// Advance the producer index by one
    pub fn advance_producer_index(&mut self) {
        self.producer_index.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    // descriptors

    /// Obtain an immutable reference to the contents of the nth descriptor
    pub const fn get_nth_descriptor(&self, index: usize) -> &D {
        &self.descriptors[index]
    }

    /// Obtain a mutable reference to the contents of the nth descriptor
    pub fn get_nth_descriptor_mut(&mut self, index: usize) -> &mut D {
        &mut self.descriptors[index]
    }

    //
    // utilities
    //

    /// Checks if a consumer can consume an element out of this ring
    pub fn can_consume(&self) -> bool {
        self.get_consumer_index() != self.get_producer_index()
    }

    /// Checks if a producer can produce an element to this ring
    pub fn can_produce(&self) -> bool {
        ((self.get_producer_index() + 1) & self.num_elements_mask()) != self.get_consumer_index()
    }

}
impl<'a> XDPRing<'a, libc::xdp_desc> {
    /// Obtain the immutable memory slice associated with the nth descriptor
    pub const fn get_nth_slice(&self, index: usize, umem: &Umem) -> &[u8] {
        let descriptor = self.get_nth_descriptor(index);
        unsafe {
            std::slice::from_raw_parts(
                umem.memory_ptr().byte_add(descriptor.addr as _),
                descriptor.len as _,
            )
        }
    }
    /// Obtain the mutable memory slice associated with the nth descriptor, eventually updating its offset and length beforehand
    pub fn get_nth_slice_mut(&mut self, index: usize, umem: &Umem, set_offset: Option<u64>, set_length: Option<usize>) -> &mut [u8] {
        let descriptor = self.get_nth_descriptor_mut(index);
        if let Some(offset) = set_offset {
            descriptor.addr = offset;
        }
        if let Some(length) = set_length {
            descriptor.len = length as _;
        }
        unsafe {
            std::slice::from_raw_parts_mut(
                umem.memory_ptr().cast_mut().byte_add(descriptor.addr as _),
                descriptor.len as _,
            )
        }
    }
}
impl<'a> XDPRing<'a, u64> {
    /// Gets the umem offset associated with the nth descriptor
    pub const fn get_nth_umem_offset(&self, index: usize) -> u64 {
        *self.get_nth_descriptor(index)
    }

    /// Sets the umem offset associated with the nth descriptor
    pub fn set_nth_umem_offset(&mut self, index: usize, umem_offset: u64) {
        *self.get_nth_descriptor_mut(index) = umem_offset
    }

    /// Produces to the ring one umem offset
    /// 
    /// Check [`Self::can_produce`] beforehand!
    pub fn produce_umem_offset(&mut self, umem_offset: u64) {
        self.set_nth_umem_offset(self.get_producer_index() as _, umem_offset);
        self.advance_producer_index();
    }
}
impl<'a, D> Drop for XDPRing<'a, D> {
    fn drop(&mut self) {
        unsafe {
            let ptr = self.descriptors.as_mut_ptr();
            libc::munmap(ptr.cast(), self.mmap_size);
        }
    }
}
