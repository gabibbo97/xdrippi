/// The Umem is a memory area accessible to both the kernel and userspace to perform their AF_XDP tasks, i.e. where xdp_desc descriptors can point to
pub struct Umem {
    // metadata
    chunk_size: usize,
    num_chunks: usize,

    // memory allocation
    allocation: std::ptr::NonNull<libc::c_void>,
}
impl Umem {
    // constants
    const CHUNK_SIZE_2K: usize = 2048;
    const CHUNK_SIZE_4K: usize = 4096;

    // constructors

    /// Create a new umem containing `num_chunks` chunks of size 2048 bytes
    pub fn new_2k(num_chunks: usize) -> Result<Self, crate::Error> {
        Self::new(Self::CHUNK_SIZE_2K, num_chunks)
    }

    /// Create a new umem containing `num_chunks` chunks of size 4096 bytes
    pub fn new_4k(num_chunks: usize) -> Result<Self, crate::Error> {
        Self::new(Self::CHUNK_SIZE_4K, num_chunks)
    }

    fn new(chunk_size: usize, num_chunks: usize) -> Result<Self, crate::Error> {
        // check chunk size
        match chunk_size {
            Self::CHUNK_SIZE_2K | Self::CHUNK_SIZE_4K => {},
            other => panic!("Chunk size {other} is not supported"),
        };

        // page size
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as usize;

        // allocate memory
        let allocation = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                chunk_size * num_chunks,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED | libc::MAP_ANONYMOUS,
                0,
                0
            )
        };
        if allocation == libc::MAP_FAILED || allocation.is_null() {
            return Err(crate::Error::MemoryAllocationFailure);
        }

        // check aligned
        assert_eq!(allocation as usize & (page_size - 1), 0);

        // zero out memory
        unsafe { libc::memset(allocation, 0, chunk_size * num_chunks); }

        // create object
        Ok(Self {
            // metadata
            chunk_size,
            num_chunks,
            // memory allocation
            allocation: unsafe { std::ptr::NonNull::new_unchecked(allocation) },
        })
    }

    // metadata

    /// How big in bytes an individual chunk is
    pub const fn chunk_size(&self) -> usize {
        self.chunk_size
    }

    /// How big is the umem allocated memory area
    pub const fn memory_size(&self) -> usize {
        self.chunk_size * self.num_chunks
    }

    /// How many chunks are contained within this umem
    pub const fn num_chunks(&self) -> usize {
        self.num_chunks
    }

    /// Given a chunk index, return the offset from the start of allocated area where that chunk starts
    pub const fn chunk_start_offset_for_index(&self, index: usize) -> u64 {
        (self.chunk_size * index) as u64
    }

    /// Given an offset, return the chunk index associated with it
    pub const fn chunk_index_for_offset(&self, offset: u64) -> usize {
        offset as usize / self.chunk_size
    }

    // memory

    /// Obtain a pointer to the umem allocation
    /// 
    /// # Safety
    /// This function is __unsafe__ because it's always possible to cast a *const pointer into a *mut pointer
    pub const unsafe fn memory_ptr(&self) -> *const u8 {
        self.allocation.as_ptr().cast()
    }
}
impl Drop for Umem {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.allocation.as_ptr(), self.memory_size()) };
    }
}
unsafe impl Send for Umem {}
unsafe impl Sync for Umem {}
