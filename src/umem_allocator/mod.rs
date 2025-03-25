use std::sync::Arc;

use crate::Umem;

mod queue; pub use queue::ConcurrentQueueAllocator;

pub type DefaultAllocator = ConcurrentQueueAllocator;

/// A Umem allocator
pub trait UmemAllocator {
    /// Create an allocator prepopulated with all the chunks in the provided umem
    fn for_umem(umem: Arc<Umem>) -> Self
    where
        Self: Sized;

    /// Get the umem utilized by this allocator
    fn umem_reference(&self) -> &Umem;
    
    /// Try to allocate a chunk, returning its index
    fn try_allocate(&self) -> Option<usize>;

    /// Try to release a chunk back to the allocator, provided by its index
    fn try_release(&self, index: usize) -> bool;

    /// Release a chunk back to the allocator, provided by its index
    /// 
    /// Panics on failure
    fn release(&self, index: usize) {
        if ! self.try_release(index) {
            panic!("Failed releasing chunk at index {index}");
        }
    }

    /// Try to release a chunk back to the allocator, provided by its offset in the umem area
    fn try_release_offset(&self, offset: u64) -> bool {
        let index = self.umem_reference().chunk_index_for_offset(offset);
        self.try_release(index)
    }

    /// Release a chunk back to the allocator, provided by its offset in the umem area
    /// 
    /// Panics on failure
    fn release_offset(&self, offset: u64) {
        if ! self.try_release_offset(offset) {
            panic!("Failed releasing chunk at offset {offset}");
        }
    }

    /// Estimates the available free slots
    fn num_available(&self) -> Option<usize> {
        None
    }

}
