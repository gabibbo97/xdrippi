use std::sync::{atomic::{AtomicU64, AtomicUsize}, Arc};

use crate::Umem;

use super::UmemAllocator;

pub struct AtomicBitSetAllocator {
    umem: Arc<Umem>,
    storage: Box<[AtomicU64]>,
    // Hint for next word that might have free slots
    next_word_hint: AtomicUsize,
}
impl UmemAllocator for AtomicBitSetAllocator {
    fn for_umem(umem: Arc<Umem>) -> Self
    where
        Self: Sized {
        assert_eq!(umem.num_chunks() % 64, 0, "Umem number of chunks ({}) is not divisible by 64", umem.num_chunks());
        
        let storage = (0..umem.num_chunks() / 64)
            .map(|_| AtomicU64::new(0))
            .collect();

        Self {
            umem,
            storage,
            next_word_hint: AtomicUsize::new(0),
        }
    }

    fn umem_reference(&self) -> &Umem {
        &self.umem
    }

    fn try_allocate(&self) -> Option<usize> {
        for offset in 0..self.storage.len() {
            // get word index
            let word_index = (self.next_word_hint.load(std::sync::atomic::Ordering::Relaxed) + offset) % self.storage.len();

            // load current value
            let mut word = self.storage[word_index].load(std::sync::atomic::Ordering::Relaxed);
            
            // skip full words
            if word == u64::MAX {
                continue;
            }

            // try to set a bit, starting from the first position which is not a one
            // e.g. 1 1 1 1 0 0 0 0 => leading_ones is 4
            //      0 1 2 3 4 5 6 7
            //              ^
            loop {
                // assemble bit mask
                let bit_index = word.leading_ones();
                let mask = 1_u64 << (63 - bit_index);

                // allocate
                let allocated_word = word | mask;

                // atomically compare and swap
                match self.storage[word_index].compare_exchange_weak(
                    word,
                    allocated_word,
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::Relaxed
                ) {
                    Ok(..) => {
                        // update hint
                        if allocated_word == u64::MAX {
                            self.next_word_hint.fetch_min((word_index+1) % self.storage.len(), std::sync::atomic::Ordering::Relaxed);
                        } else {
                            self.next_word_hint.fetch_min(word_index, std::sync::atomic::Ordering::Relaxed);
                        }

                        // return
                        return Some(word_index * 64 + bit_index as usize);
                    },
                    Err(new_word) => {
                        word = new_word;
                    }
                }

                // exit if the word is full
                if word == u64::MAX {
                    break
                }
            }
        }
        None
    }

    fn try_release(&self, index: usize) -> bool {
        // get the indexes
        let word_index = index / 64;
        let bit_index = index - word_index * 64;

        // do a bounds check
        if word_index >= self.storage.len() {
            return false;
        }

        // calculate mask
        let mask = 1_u64 << (63 - bit_index);
        let neg_mask = !mask;

        // deallocate
        let prev_value = self.storage[word_index].fetch_and(neg_mask, std::sync::atomic::Ordering::SeqCst);

        // update next word hint
        self.next_word_hint.fetch_min(word_index, std::sync::atomic::Ordering::Relaxed);

        (prev_value & mask) > 0
    }

    fn num_available(&self) -> Option<usize> {
        Some(
            self.storage.iter()
                .map(|atomic| atomic.load(std::sync::atomic::Ordering::Relaxed))
                .map(|number| 64 - number.count_ones() as usize)
                .sum()
        )
    }

    fn num_allocated(&self) -> Option<usize> {
        Some(
            self.storage.iter()
                .map(|atomic| atomic.load(std::sync::atomic::Ordering::Relaxed))
                .map(|number| number.count_ones() as usize)
                .sum()
        )
    }

}
impl std::fmt::Debug for AtomicBitSetAllocator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "AtomicBitSetAllocator (storage = [")?;
        for (word_index, word) in self.storage.iter().enumerate() {
            let word = word.load(std::sync::atomic::Ordering::Relaxed);
            write!(f, " ")?;
            for i in 0..64 {
                let mask = 1 << (63 - i);
                write!(f, "{}", if word & mask == 0 { '0' } else { '1' })?;
            }
            writeln!(f, " => {word_index}")?;
        }
        writeln!(f, "])")
    }
}

#[cfg(test)]
mod tests {
    use crate::umem_allocator::tests::crunch_allocator;
    use super::AtomicBitSetAllocator;

    #[test]
    fn test_atomics_allocator() {
        crunch_allocator::<AtomicBitSetAllocator>();
    }
}
