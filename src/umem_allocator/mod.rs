use std::sync::Arc;

use crate::Umem;

mod atomics;
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

    /// Estimates the allocated slots
    fn num_allocated(&self) -> Option<usize> {
        None
    }

}

#[cfg(test)]
mod tests {
    use std::sync::{atomic::AtomicBool, Arc};

    use crate::Umem;

    use super::UmemAllocator;

    pub(crate) fn benchmark_allocator<A: UmemAllocator + Send + Sync>() {
        for n_slots in [ 1024, 2048, 4096, 8192 ] {
            for n_threads in [ 1, 2, 4, 8, 16, 32 ] {
                benchmark_allocator_run::<A>(n_slots, n_threads);
            }
        }
    }

    pub(crate) fn benchmark_allocator_run<A: UmemAllocator + Send + Sync>(
        n_slots: usize,
        n_threads: usize,
    ) {
        // settings
        let n_slots_per_thread = n_slots / n_threads;

        // create umem and allocator
        let umem = Arc::new(Umem::new_2k(n_slots).unwrap());
        let allocator = Arc::new(A::for_umem(umem));

        // allocate from multiple threads
        std::thread::scope(|scope| {
            // prepare stop signal
            let mut stop_signal = Arc::new(AtomicBool::new(false));

            // spawn threads
            let barrier = Arc::new(std::sync::Barrier::new(n_threads));
            let mut handles = (0..n_threads)
                .map(|_| {
                    let allocator = allocator.clone();
                    let barrier = barrier.clone();
                    let stop_signal = stop_signal.clone();
                    scope.spawn(move || {
                        let mut positions = Vec::with_capacity(n_slots_per_thread);
                        barrier.wait();
                        let t0 = std::time::Instant::now();
                        let mut n_allocs = 0_usize;
                        let mut n_deallocs = 0_usize;
                        while ! stop_signal.load(std::sync::atomic::Ordering::Relaxed) {
                            for _ in 0..n_slots_per_thread {
                                positions.push(allocator.try_allocate().unwrap());
                                n_allocs += 1;
                            }
                            for position in positions.drain(..) {
                                assert!(allocator.try_release(position));
                                n_deallocs += 1;
                            }
                        }
                        let delta_t = t0.elapsed().as_secs_f64();
                        (n_allocs as f64 / delta_t, n_deallocs as f64 / delta_t)
                    })
                })
                .collect::<Vec<_>>();

            // send stop
            std::thread::sleep(std::time::Duration::from_secs(1));
            stop_signal.store(true, std::sync::atomic::Ordering::Relaxed);

            // wait for benchmark results
            let mut cumulative_n_allocs_per_second = 0.0;
            let mut cumulative_n_deallocs_per_second = 0.0;
            for handle in handles {
                let (n_allocs_per_second, n_deallocs_per_second) = handle.join().unwrap();
                cumulative_n_allocs_per_second += n_allocs_per_second;
                cumulative_n_deallocs_per_second += n_deallocs_per_second;
            }

            // return
            (cumulative_n_allocs_per_second, cumulative_n_deallocs_per_second)
        });
    }

    pub(crate) fn crunch_allocator<A: UmemAllocator + Send + Sync>() {
        for n_slots in [ 1024, 2048, 4096, 8192 ] {
            for n_threads in [ 1, 2, 4, 8, 16, 32 ] {
                crunch_allocator_run::<A>(n_slots, n_threads);
            }
        }
    }

    pub(crate) fn crunch_allocator_run<A: UmemAllocator + Send + Sync>(
        n_slots: usize,
        n_threads: usize,
    ) {
        // settings
        let n_slots_per_thread = n_slots / n_threads;

        // create umem and allocator
        let umem = Arc::new(Umem::new_2k(n_slots).unwrap());
        let allocator = Arc::new(A::for_umem(umem));

        // allocate from multiple threads
        std::thread::scope(|scope| {
            // spawn threads
            let barrier = Arc::new(std::sync::Barrier::new(n_threads));
            let mut handles = (0..n_threads)
                .map(|_| {
                    let allocator = allocator.clone();
                    let barrier = barrier.clone();
                    scope.spawn(move || {
                        let mut positions = Vec::with_capacity(n_slots_per_thread);
                        barrier.wait();
                        for _ in 0..n_slots_per_thread {
                            positions.push(allocator.try_allocate().unwrap());
                        }
                        positions
                    })
                })
                .collect::<Vec<_>>();

            // wait for termination
            let positions = handles.drain(..)
                .map(|handle| handle.join().unwrap())
                .reduce(|acc, x| [acc,x].concat())
                .unwrap();

            // assert we allocated the whole umem, uniquely (due to the set)
            assert_eq!(positions.len(), n_slots);

            // assert allocator is empty
            assert_eq!(allocator.try_allocate(), None);

            // give back all allocations
            for position in positions {
                assert!(allocator.try_release(position), "Failed releasing position {position}");
            }

            // check the number of available slots
            match allocator.num_available() {
                Some(x) => assert_eq!(x, n_slots),
                None => {},
            }
        });
    }
}
