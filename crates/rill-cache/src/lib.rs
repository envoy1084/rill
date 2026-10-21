//! The regional cache tier.
//!
//! Objects never change, so a cache near its clients can serve them
//! without asking the origin anything: it checks each request's read grant
//! against the origin's published grant keys, serves the object from local
//! disk, and on a miss fetches it once from the origin with the client's
//! own credentials, however many clients asked at the same time.
//!
//! * [`disk`]: the bounded, least-recently-used store on local disk.
//! * [`keys`]: the origin's grant keys.
//! * [`origin`]: fetching from the origin's object protocol.
//! * [`server`]: the object protocol this tier serves.

mod coalesce;
pub mod config;
pub mod disk;
pub mod error;
pub mod keys;
pub mod origin;
pub mod redactions;
pub mod server;

pub use config::CacheConfig;
pub use error::{Error, Result};
pub use server::{router, serve, serve_on, Cache, CacheStats};
