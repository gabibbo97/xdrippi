use std::sync::Arc;

use crate::Umem;

use super::UmemAllocator;

/// A very simple umem allocator adding available chunks to an internal list
pub struct ConcurrentQueueAllocator {
    umem: Arc<Umem>,
    available_chunks: crossbeam::queue::ArrayQueue<usize>,
}
impl UmemAllocator for ConcurrentQueueAllocator {
    fn for_umem(umem: Arc<Umem>) -> Self
    where
        Self: Sized {
        // make the chunk list
        let available_chunks = crossbeam::queue::ArrayQueue::new(umem.num_chunks());
        for i in 0..umem.num_chunks() {
            available_chunks.push(i).unwrap();
        }
        Self {
            umem,
            available_chunks
        }
    }

    fn umem_reference(&self) -> &Umem {
        &self.umem
    }

    fn try_allocate(&self) -> Option<usize> {
        self.available_chunks.pop()
    }

    fn try_release(&self, index: usize) -> bool {
        // check
        if index >= self.umem.num_chunks() {
            return false;
        }
        // try to push
        if let Err(_) = self.available_chunks.push(index) {
            return false;
        }
        true
    }

    fn num_allocated(&self) -> Option<usize> {
        Some(self.available_chunks.len())
    }

    fn num_available(&self) -> Option<usize> {
        Some(self.available_chunks.capacity() - self.available_chunks.len())
    }

}