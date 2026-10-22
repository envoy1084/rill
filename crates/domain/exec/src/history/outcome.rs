//! How one run of a test or step ended.

use rill_types::{ObjectHash, Timestamp};
use serde::{Deserialize, Serialize};

/// The outcome of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestOutcome {
    Passed,
    Failed,
    Skipped,
    /// It could not run (infrastructure); says nothing about the test.
    Error,
}

impl TestOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            TestOutcome::Passed => "passed",
            TestOutcome::Failed => "failed",
            TestOutcome::Skipped => "skipped",
            TestOutcome::Error => "error",
        }
    }

    /// Whether the outcome says something about the test itself.
    pub fn is_verdict(self) -> bool {
        matches!(self, TestOutcome::Passed | TestOutcome::Failed)
    }
}

/// One recorded run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestRun {
    /// `step:<name>[#shard]` or `test:<class>::<name>`.
    pub target: String,
    pub outcome: TestOutcome,
    /// The test runner retried and it passed: a flake by its own account.
    pub retried: bool,
    pub duration_ms: Option<u64>,
    /// The action it ran in; the same action is the same inputs.
    pub action: Option<ObjectHash>,
    pub worker: Option<String>,
    /// The version verified.
    pub version: ObjectHash,
    /// Verified after landing rather than before.
    pub trunk: bool,
    pub at: Timestamp,
}

impl rill_types::Record for TestRun {
    const TAG: &'static str = "test-run";
}
