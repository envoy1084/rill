//! What verifying a tree found.

use std::time::Duration;

use rill_types::{ObjectHash, Timestamp};

use crate::history::{TestCaseResult, TestOutcome, TestRun};
use crate::remote::BlobDigest;

/// How one step (or shard) ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepStatus {
    Passed,
    Failed {
        exit_code: i32,
    },
    TimedOut,
    /// The action could not run (a missing input, a lost worker).
    Error(String),
    /// Not run: the change touched none of its inputs.
    Unaffected,
    /// Not run: it does not gate the change's risk class.
    NotGating,
}

impl StepStatus {
    /// Whether the step lets the tree pass.
    pub fn is_ok(&self) -> bool {
        matches!(
            self,
            StepStatus::Passed | StepStatus::Unaffected | StepStatus::NotGating
        )
    }
}

/// The result of one step, or one shard of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepResult {
    pub step: String,
    /// `(index, total)` for a shard.
    pub shard: Option<(u32, u32)>,
    pub status: StepStatus,
    /// Served from the action cache rather than run.
    pub cached: bool,
    pub action: Option<BlobDigest>,
    pub stdout: Option<BlobDigest>,
    pub stderr: Option<BlobDigest>,
    pub worker: Option<String>,
    pub duration: Option<Duration>,
    /// Test cases from the step's JUnit report.
    pub tests: Vec<TestCaseResult>,
    /// Output files it produced (artifacts), by path.
    pub outputs: Vec<(String, BlobDigest)>,
    /// A probe found the step non-hermetic: it ran uncached.
    pub nonhermetic: bool,
}

impl StepResult {
    /// A step that did not run.
    pub fn skipped(step: &str, status: StepStatus) -> Self {
        StepResult {
            step: step.to_string(),
            shard: None,
            status,
            cached: false,
            action: None,
            stdout: None,
            stderr: None,
            worker: None,
            duration: None,
            tests: Vec::new(),
            outputs: Vec::new(),
            nonhermetic: false,
        }
    }

    /// `"unit"` or `"unit" shard 2/4`.
    pub fn label(&self) -> String {
        match self.shard {
            Some((index, total)) => format!("{:?} shard {}/{total}", self.step, index + 1),
            None => format!("{:?}", self.step),
        }
    }
}

/// The result of verifying a tree.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VerificationOutcome {
    /// Every step passed or did not need to run.
    pub green: bool,
    pub steps: Vec<StepResult>,
    /// The end of the first failure's output.
    pub excerpt: Option<String>,
    /// The run recording it, when runs are kept.
    pub run: Option<String>,
}

impl VerificationOutcome {
    /// The first step that did not pass.
    pub fn first_failure(&self) -> Option<&StepResult> {
        self.steps.iter().find(|s| !s.status.is_ok())
    }

    /// One line for logs and land statuses.
    pub fn summary(&self) -> String {
        let count = |f: &dyn Fn(&StepResult) -> bool| self.steps.iter().filter(|s| f(s)).count();
        let passed = count(&|s| s.status == StepStatus::Passed);
        let cached = count(&|s| s.status == StepStatus::Passed && s.cached);
        let unaffected = count(&|s| s.status == StepStatus::Unaffected);
        let not_gating = count(&|s| s.status == StepStatus::NotGating);
        let mut parts = vec![format!("{passed} passed ({cached} cached)")];
        if unaffected > 0 {
            parts.push(format!("{unaffected} unaffected"));
        }
        if not_gating > 0 {
            parts.push(format!("{not_gating} not gating"));
        }
        let tally = parts.join(", ");
        match self.first_failure() {
            None => tally,
            Some(failed) => {
                let what = match &failed.status {
                    StepStatus::Failed { exit_code } => {
                        format!("failed with exit code {exit_code}")
                    }
                    StepStatus::TimedOut => "timed out".to_string(),
                    StepStatus::Error(message) => format!("could not run: {message}"),
                    other => format!("ended {other:?}"),
                };
                format!("step {} {what}; {tally}", failed.label())
            }
        }
    }
}

impl VerificationOutcome {
    /// The runs to record in the test history for verifying `version`:
    /// every step or shard that ran (not ones served from the cache, which
    /// were recorded when they ran) and each of its test cases.
    pub fn test_runs(&self, version: ObjectHash, trunk: bool, at: Timestamp) -> Vec<TestRun> {
        let mut runs = Vec::new();
        for step in self.steps.iter().filter(|s| !s.cached) {
            let outcome = match &step.status {
                StepStatus::Passed => TestOutcome::Passed,
                StepStatus::Failed { .. } | StepStatus::TimedOut => TestOutcome::Failed,
                StepStatus::Error(_) => TestOutcome::Error,
                StepStatus::Unaffected | StepStatus::NotGating => continue,
            };
            let target = match step.shard {
                Some((index, total)) => format!("step:{}#{}/{total}", step.step, index + 1),
                None => format!("step:{}", step.step),
            };
            let action = step.action.map(|a| a.hash);
            runs.push(TestRun {
                target,
                outcome,
                retried: false,
                duration_ms: step.duration.map(|d| d.as_millis() as u64),
                action,
                worker: step.worker.clone(),
                version,
                trunk,
                at,
            });
            for case in &step.tests {
                runs.push(TestRun {
                    target: format!("test:{}", case.id),
                    outcome: case.outcome,
                    retried: case.retried,
                    duration_ms: case.duration_ms,
                    action,
                    worker: step.worker.clone(),
                    version,
                    trunk,
                    at,
                });
            }
        }
        runs
    }
}
