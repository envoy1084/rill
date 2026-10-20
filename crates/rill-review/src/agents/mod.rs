//! Agent orchestration: reviewers that post findings and vote, and the
//! resolver interface for conflict-resolving agents.

mod context;
mod heuristic;
mod pool;
mod resolver;
mod reviewer;

pub use heuristic::{HeuristicConfig, HeuristicReviewer, HEURISTIC_REVIEWER_NAME};
pub use pool::{PoolConfig, PoolReport, ReviewerOutcome, ReviewerPool, REVIEW_LABEL};
pub use resolver::{ConflictTerms, Resolution, Resolver};
pub use reviewer::{Budget, FileDiff, Finding, ReviewContext, Reviewer, TouchedLine};
