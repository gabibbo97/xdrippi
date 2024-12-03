use std::sync::Arc;

use crate::Umem;

/// A very simple umem allocator adding available chunks to an internal list
pub struct UmemAllocator {
    umem: Arc<Umem>,
    available_chunks: crossbeam::queue::ArrayQueue<usize>,
}
impl UmemAllocator {
    // constructor

    /// Create an allocator prepopulated with all the chunks in the provided umem
    pub fn for_umem(umem: Arc<Umem>) -> Self {
        let available_chunks = crossbeam::queue::ArrayQueue::new(umem.num_chunks());
        for i in 0..umem.num_chunks() {
            available_chunks.push(i).unwrap();
        }
        Self {
            umem,
            available_chunks
        }
    }

    // methods

    /// Try to allocate a chunk, returning its index
    #[tracing::instrument(skip_all, level = tracing::Level::TRACE, ret)]
    pub fn try_allocate(&self) -> Option<usize> {
        self.available_chunks.pop()
    }

    /// Release a chunk back to the allocator, provided by its index
    #[tracing::instrument(skip(self), level = tracing::Level::TRACE, ret)]
    pub fn release(&self, index: usize) {
        if index >= self.umem.num_chunks() {
            panic!("tried to release block {index} for an umem of size {}", self.umem.num_chunks());
        }
        if let Err(err) = self.available_chunks.push(index) {
            tracing::error!(num_available = self.num_available(), element = err, "could not push (queue full)");
            panic!("release failed");
        }
    }

    /// Release a chunk back to the allocator, provided by its offset in the umem area
    #[tracing::instrument(skip(self))]
    pub fn release_offset(&self, offset: u64) {
        let i = self.umem.chunk_index_for_offset(offset);
        tracing::trace!(chunk_index = i);
        self.release(i);
    }

    /// Estimates the available free slots
    pub fn num_available(&self) -> usize {
        self.available_chunks.len()
    }

}
