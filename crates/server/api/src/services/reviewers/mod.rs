//! `rill.v1.ReviewerService`: reviewer and resolver agents lease jobs from
//! their queues and report results.

mod lease;
mod packet;
mod resolve_jobs;
mod review_jobs;
mod runs;
mod service;
mod stats;

pub use service::ReviewerSvc;
