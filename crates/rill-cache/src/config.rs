//! How a cache process runs.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

/// A cache tier process.
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Where clients reach it.
    pub listen: SocketAddr,
    /// The origin's HTTP listener, e.g. `https://rill.internal:7421`.
    pub origin: String,
    /// The directory objects are kept in.
    pub dir: PathBuf,
    /// The most bytes kept; least recently used objects go first.
    pub max_bytes: u64,
    /// How often the origin's grant keys are reread.
    pub key_refresh: Duration,
}

impl CacheConfig {
    /// A cache of `origin` in `dir` holding up to `max_bytes`.
    pub fn new(origin: impl Into<String>, dir: impl Into<PathBuf>, max_bytes: u64) -> Self {
        CacheConfig {
            listen: SocketAddr::from(([127, 0, 0, 1], 7422)),
            origin: origin.into(),
            dir: dir.into(),
            max_bytes,
            key_refresh: Duration::from_secs(60),
        }
    }
}
