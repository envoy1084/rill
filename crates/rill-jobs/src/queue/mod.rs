//! A durable queue in one repository.

mod claiming;
mod failure;
mod handle;

pub use handle::{metric, Queue, QueueStats, DEFAULT_SHARDS};
