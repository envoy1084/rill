//! Attempts, backoff and lease length.

use std::time::Duration;

/// How a queue retries and how long a claim lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Attempts before a job goes to the dead-letter queue.
    pub max_attempts: u32,
    /// Delay before the first retry; doubles with each attempt.
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
    /// The delay before attempt `attempt` (the first retry is attempt 1).
    pub fn delay(&self, attempt: u32) -> Duration {
        let factor = 1u32 << attempt.saturating_sub(1).min(20);
        self.base_delay.saturating_mul(factor).min(self.max_delay)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_up_to_the_cap() {
        let policy = RetryPolicy::default();
        let delays: Vec<_> = (1..=10).map(|a| policy.delay(a).as_secs()).collect();
        assert_eq!(delays, [1, 2, 4, 8, 16, 32, 64, 128, 256, 300]);
        assert_eq!(policy.delay(u32::MAX), policy.max_delay);
    }
}
