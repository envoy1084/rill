//! `rill.v1.AttentionService`: the inbox, sampled audits, batch review and
//! intent digests.

mod audits;
mod batch;
mod inbox;
mod service;

pub use service::AttentionSvc;
