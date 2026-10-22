//! Per-principal request rate limits (GCRA, from `governor`).

use std::num::NonZeroU32;
use std::time::Duration;

use governor::clock::{Clock, DefaultClock};
use governor::{DefaultKeyedRateLimiter, Quota};

use crate::config::RateLimitConfig;
use crate::error::{Error, Result};

/// How often idle principals' state is dropped.
pub const SWEEP_EVERY: Duration = Duration::from_secs(60);

/// A token bucket per principal.
pub struct RateLimits {
    limiter: DefaultKeyedRateLimiter<String>,
    clock: DefaultClock,
}

impl RateLimits {
    /// Limits per `config`; `None` when rate limiting is off.
    pub fn new(config: &RateLimitConfig) -> Option<Self> {
        let rate = NonZeroU32::new(config.per_principal_per_sec)?;
        let burst = NonZeroU32::new(config.burst).unwrap_or(rate);
        Some(RateLimits {
            limiter: DefaultKeyedRateLimiter::keyed(Quota::per_second(rate).allow_burst(burst)),
            clock: DefaultClock::default(),
        })
    }

    /// Admit one request from `principal`, or say when it may retry.
    pub fn check(&self, principal: &str) -> Result<()> {
        self.limiter
            .check_key(&principal.to_owned())
            .map_err(|not_until| {
                let wait = not_until.wait_time_from(self.clock.now());
                Error::RateLimited {
                    principal: principal.to_owned(),
                    retry_after_secs: wait.as_secs().max(1),
                }
            })
    }

    /// Forget principals whose buckets are full again.
    pub fn sweep(&self) {
        self.limiter.retain_recent();
        self.limiter.shrink_to_fit();
    }
}
