//! Test history: the outcome of every step and test case a verification
//! ran, per repository, and flake scores computed from it.

mod junit;
mod outcome;
mod quarantine;
mod score;
mod store;

pub use junit::{parse_junit, TestCaseResult};
pub use outcome::{TestOutcome, TestRun};
pub use quarantine::Quarantine;
pub use score::{FlakeStats, HALF_LIFE_MS};
pub use store::{FlakyTarget, TestHistory, RUNS_KEPT};
