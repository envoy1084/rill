//! The host's shared object cache: content-addressed files on disk with
//! LRU eviction and pins, plus the fetch path that fills it.

mod fetch;
mod lru;
mod store;
mod trees;

pub use fetch::{local_digest, Content};
pub use lru::Lru;
pub(crate) use store::write_atomically;
pub use store::{CacheStats, ObjectCache};
pub use trees::TreeCache;
