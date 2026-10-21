//! Durable job queues kept in the metadata store.
//!
//! A job is enqueued by the transaction that makes it necessary, so it
//! exists exactly when that change committed. Workers claim jobs under a
//! lease and renew it while they run; a worker that dies stops renewing,
//! the reaper puts its job back, and the claim token it held no longer
//! matches, so a late write from it is refused.
//!
//! * [`job`]: jobs and their stored records.
//! * [`retry`]: attempts, backoff and lease length.
//! * [`queue`]: enqueue, claim, renew, complete, fail and reap.
//! * [`worker`]: a runtime that drives a handler over a queue.

pub mod claim;
pub mod error;
pub mod job;
pub mod queue;
pub mod retry;
pub mod worker;

pub use claim::Claim;
pub use error::{Error, Result};
pub use job::{DeadJob, Job, JobSpec, Priority};
pub use queue::{metric, Queue, QueueStats, DEFAULT_SHARDS};
pub use retry::RetryPolicy;
pub use worker::{Handler, JobFailure, Worker};
