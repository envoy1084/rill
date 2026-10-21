//! Request rate limits.

use serde::{Deserialize, Serialize};

/// `[rate_limit]`: a token bucket per authenticated principal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RateLimitConfig {
    /// Sustained requests per second per principal; 0 turns limits off.
    pub per_principal_per_sec: u32,
    /// Requests a principal may send at once after being idle.
    pub burst: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        RateLimitConfig {
            per_principal_per_sec: 1000,
            burst: 2000,
        }
    }
}
