//! `rill.v1.ReviewService`: a thin mapping onto `rill_review::ReviewService`,
//! with resource names and pages.

mod feedback;
mod mechanics;
mod owners;
mod ownership;
mod reviews;
mod service;
mod watch;

pub use service::ReviewSvc;
pub use watch::ReviewEventStream;
