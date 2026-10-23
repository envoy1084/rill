//! The reviewer fleet's server side: the context a reviewing agent is
//! given, the checks its result must pass before it is posted as that
//! agent, the record of every run, and reviewers' track records.

mod calibration;
mod context;
mod results;
mod reviewer;
mod routing;
mod runs;

pub use calibration::StatsMap;
pub use results::{check_result, PostedResult, ReviewResult, MAX_BODY, MAX_FINDINGS};
pub use reviewer::{Budget, FileDiff, Finding, ReviewContext, TouchedLine};
pub use runs::{ReviewRun, RunKind, RunStatus, MAX_RUNS};
