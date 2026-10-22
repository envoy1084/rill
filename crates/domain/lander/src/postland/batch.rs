//! How many landings one sweep verifies together: more while trunk stays
//! green, so verification keeps pace with landing, and fewer after red,
//! so the next break is found in fewer rounds.

use std::sync::atomic::{AtomicU64, Ordering};

/// The size a sweep starts at, below the maximum.
const START: u64 = 4;

/// The adaptive batch size of one worker.
#[derive(Debug)]
pub struct BatchSize {
    current: AtomicU64,
    max: u64,
}

impl BatchSize {
    /// Batches of at most `max` landings.
    pub fn new(max: u64) -> Self {
        let max = max.max(1);
        BatchSize {
            current: AtomicU64::new(START.min(max)),
            max,
        }
    }

    pub fn get(&self) -> u64 {
        self.current.load(Ordering::Relaxed)
    }

    /// Double after a green batch, halve after a red one.
    pub fn after(&self, green: bool) {
        let now = self.get();
        let next = match green {
            true => (now * 2).min(self.max),
            false => (now / 2).max(1),
        };
        self.current.store(next, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grows_while_green_to_the_maximum_and_halves_on_red() {
        let size = BatchSize::new(32);
        assert_eq!(size.get(), 4);
        for _ in 0..5 {
            size.after(true);
        }
        assert_eq!(size.get(), 32);
        size.after(false);
        size.after(false);
        assert_eq!(size.get(), 8);
        for _ in 0..10 {
            size.after(false);
        }
        assert_eq!(size.get(), 1);
        assert_eq!(BatchSize::new(2).get(), 2);
    }
}
