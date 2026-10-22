//! Attempts, backoff and lease length.

use std::time::Duration;

/// How a queue retries and how long a claim lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Attempts before a job goes to the dead-letter queue.
    pub max_attempts: u32,
    /// Delay before the first retry; doubles with each attempt, and each
    /// delay is jittered between half and all of it.
    pub base_delay: Duration,
    pub max_delay: Duration,
    /// How long a claim lasts without renewal.
    pub lease: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy {
            max_attempts: 5,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(300),
            lease: Duration::from_secs(30),
        }
    }
}

impl RetryPolicy {
    /// The delay before attempt `attempt` (the first retry is attempt 1):
    /// a random point in the upper half of the exponential delay, so jobs
    /// that failed together do not all retry at the same moment.
    pub fn delay(&self, attempt: u32) -> Duration {
        let factor = 1u32 << attempt.saturating_sub(1).min(20);
        let full = self.base_delay.saturating_mul(factor).min(self.max_delay);
        full.mul_f64(rand::random_range(0.5..=1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_up_to_the_cap_with_jitter() {
        let policy = RetryPolicy::default();
        for (attempt, full) in (1..=10).zip([1, 2, 4, 8, 16, 32, 64, 128, 256, 300]) {
            let full = Duration::from_secs(full);
            let delay = policy.delay(attempt);
            assert!(delay >= full / 2 && delay <= full, "{attempt}: {delay:?}");
        }
        assert!(policy.delay(u32::MAX) <= policy.max_delay);
        let spread: std::collections::BTreeSet<_> = (0..50).map(|_| policy.delay(6)).collect();
        assert!(spread.len() > 10, "jobs failing together retry apart");
    }
}
