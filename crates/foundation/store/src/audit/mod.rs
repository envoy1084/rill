//! The audit log in the metadata store. Records are queued in the
//! transaction of the action they describe (or from the logs that already
//! record it); a chainer then gives each its place in its org's hash chain,
//! and roots seal the chain periodically.

mod chain;
mod cursors;
mod queue;
mod roots;
mod store;

pub use chain::CHAIN_BATCH;
pub use store::AuditStore;
