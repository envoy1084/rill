//! The object protocol at the cache tier: `GET`/`HEAD /o/{repo}/{kind}/{hash}`
//! with a read grant in `x-rill-grant` (or `?grant=`), and `?via=` for a
//! chunk. The grant decides; the `{repo}` segment is the client's name for
//! the repository and is not trusted. A grant is a bearer credential here:
//! whoever holds it may fetch what it names until it expires.

mod cache;
mod metrics;
mod objects;
mod serve;

pub use cache::{Cache, CacheStats};
pub use serve::{router, serve, serve_on};
