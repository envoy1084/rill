//! Where blob reads were served from, and how fast: the regional cache or
//! the origin, plus reads the cache could not serve.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use hdrhistogram::Histogram;
use parking_lot::Mutex;

/// Microseconds the histograms span.
const MAX_MICROS: u64 = 600_000_000;

fn histogram() -> Histogram<u64> {
    Histogram::new_with_bounds(1, MAX_MICROS, 2).expect("static bounds are valid")
}

/// Reads of one tier.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TierStats {
    /// Reads served.
    pub reads: u64,
    /// Median latency.
    pub p50: Duration,
    /// 99th percentile latency.
    pub p99: Duration,
}

/// A snapshot of a client's blob reads, mergeable across clients.
#[derive(Debug, Clone)]
pub struct ReadStats {
    regional: Histogram<u64>,
    origin: Histogram<u64>,
    /// Reads the regional cache could not serve (answered by the origin).
    pub regional_fallbacks: u64,
}

impl Default for ReadStats {
    fn default() -> Self {
        ReadStats {
            regional: histogram(),
            origin: histogram(),
            regional_fallbacks: 0,
        }
    }
}

fn tier(h: &Histogram<u64>) -> TierStats {
    TierStats {
        reads: h.len(),
        p50: Duration::from_micros(h.value_at_quantile(0.5)),
        p99: Duration::from_micros(h.value_at_quantile(0.99)),
    }
}

impl ReadStats {
    /// Reads the regional cache served.
    pub fn regional(&self) -> TierStats {
        tier(&self.regional)
    }

    /// Reads the origin served.
    pub fn origin(&self) -> TierStats {
        tier(&self.origin)
    }

    /// Add another snapshot's reads.
    pub fn merge(&mut self, other: &ReadStats) {
        let _ = self.regional.add(&other.regional);
        let _ = self.origin.add(&other.origin);
        self.regional_fallbacks += other.regional_fallbacks;
    }
}

/// The live counters a client records into.
#[derive(Debug)]
pub(crate) struct ReadCounters {
    regional: Mutex<Histogram<u64>>,
    origin: Mutex<Histogram<u64>>,
    fallbacks: AtomicU64,
}

impl Default for ReadCounters {
    fn default() -> Self {
        ReadCounters {
            regional: Mutex::new(histogram()),
            origin: Mutex::new(histogram()),
            fallbacks: AtomicU64::new(0),
        }
    }
}

fn record(h: &Mutex<Histogram<u64>>, took: Duration) {
    let _ = h
        .lock()
        .record((took.as_micros() as u64).clamp(1, MAX_MICROS));
}

impl ReadCounters {
    pub(crate) fn regional(&self, took: Duration) {
        record(&self.regional, took);
    }

    pub(crate) fn origin(&self, took: Duration) {
        record(&self.origin, took);
    }

    pub(crate) fn fallback(&self) {
        self.fallbacks.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn snapshot(&self) -> ReadStats {
        ReadStats {
            regional: self.regional.lock().clone(),
            origin: self.origin.lock().clone(),
            regional_fallbacks: self.fallbacks.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merged_snapshots_add_their_reads() {
        let a = ReadCounters::default();
        a.regional(Duration::from_millis(2));
        a.fallback();
        let b = ReadCounters::default();
        b.regional(Duration::from_millis(8));
        b.origin(Duration::from_millis(40));
        let mut total = a.snapshot();
        total.merge(&b.snapshot());
        assert_eq!(total.regional().reads, 2);
        assert_eq!(total.origin().reads, 1);
        assert_eq!(total.regional_fallbacks, 1);
        assert!(total.origin().p50 >= Duration::from_millis(39));
    }
}
