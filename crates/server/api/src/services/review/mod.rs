//! `rill.v1.ReviewService`: a thin mapping onto `rill_review::ReviewService`
//! plus the hand-off to the lander.

mod feedback;
mod land;
mod owners;
mod reviews;
mod service;
mod watch;

pub use service::ReviewSvc;
pub use watch::ReviewEventStream;
