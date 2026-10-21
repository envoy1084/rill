//! How long queued requests wait: recent landings give the rate the queue
//! drains at, and a request's place in the serve order its ETA.

use std::collections::VecDeque;
use std::time::Duration;

use parking_lot::Mutex;

/// Landings remembered for timing.
const REMEMBERED: usize = 512;

/// Recent landings: when each landed and how long it waited in the queue.
#[derive(Default)]
pub struct Timings {
    landed: Mutex<VecDeque<(i64, i64)>>,
}

/// What [`Timings`] says now.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Eta {
    /// Mean time between recent landings.
    pub interval: Duration,
    /// Queue-to-trunk percentiles of recent landings.
    pub p50: Duration,
    pub p99: Duration,
}

impl Timings {
    pub fn new() -> Self {
        Self::default()
    }

    /// A request queued at `queued_ms` landed at `landed_ms`.
    pub fn landed(&self, queued_ms: i64, landed_ms: i64) {
        let mut landed = self.landed.lock();
        landed.push_back((landed_ms, (landed_ms - queued_ms).max(0)));
        while landed.len() > REMEMBERED {
            landed.pop_front();
        }
    }

    pub fn snapshot(&self) -> Eta {
        let landed = self.landed.lock();
        if landed.is_empty() {
            return Eta::default();
        }
        let first = landed.front().map_or(0, |l| l.0);
        let last = landed.back().map_or(0, |l| l.0);
        let interval = if landed.len() > 1 {
            ((last - first) / (landed.len() as i64 - 1)).max(0)
        } else {
            0
        };
        let mut waits: Vec<i64> = landed.iter().map(|l| l.1).collect();
        waits.sort_unstable();
        let pick = |q: f64| waits[((waits.len() - 1) as f64 * q).round() as usize];
        Eta {
            interval: Duration::from_millis(interval as u64),
            p50: Duration::from_millis(pick(0.5) as u64),
            p99: Duration::from_millis(pick(0.99) as u64),
        }
    }
}

impl Eta {
    /// How long until a request `position` places from the front (0 =
    /// next) lands at the recent rate. Unknown (zero) before any landing.
    pub fn wait(&self, position: usize) -> Duration {
        self.interval * (position as u32 + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rate_and_waits_come_from_recent_landings() {
        let timings = Timings::new();
        assert_eq!(
            timings.snapshot().wait(3),
            Duration::ZERO,
            "unknown before any landing"
        );
        for i in 0..100 {
            timings.landed(i * 100 - 500 - i, i * 100);
        }
        let eta = timings.snapshot();
        assert_eq!(eta.interval, Duration::from_millis(100));
        assert_eq!(eta.wait(4), Duration::from_millis(500));
        assert!(eta.p50 >= Duration::from_millis(500) && eta.p99 > eta.p50);
    }
}
