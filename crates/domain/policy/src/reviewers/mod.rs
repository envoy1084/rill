//! Agent reviewers: which ones a change is routed to (`.rill/review.toml`)
//! which of their approvals are independent of the author, and how far
//! their track record lets them be trusted.

pub mod config;
pub mod independence;
pub mod trust;

pub use config::{ResolverRoute, ReviewConfig, ReviewerRoute, REVIEW_CONFIG};
pub use independence::{independent_approvals, Independence};
pub use trust::{ReviewerStats, DEMOTE_BELOW, MIN_OUTCOMES};
