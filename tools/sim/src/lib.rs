//! Deterministic simulation of Rill's review and landing workflow.
//!
//! A run drives simulated agents and reviewers against the real stores,
//! review service and lander, with store faults and process restarts
//! chosen by one seed, and checks the invariants in [`oracle`] along the
//! way. The same seed always replays the same run, so a failing seed is a
//! reproducible bug report.

pub mod actors;
pub mod approvals;
pub mod event_log;
pub mod faults;
pub mod jobs;
pub mod kill;
pub mod known;
pub mod oracle;
pub mod processes;
pub mod rng;
pub mod run;
pub mod scenario;
pub mod world;
pub mod zombie;

pub use known::{known, KnownIssue, KNOWN_ISSUES};
pub use oracle::{Invariant, Violation};
pub use run::{run, Report};
pub use scenario::Scenario;
