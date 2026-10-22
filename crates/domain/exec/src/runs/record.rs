//! A verification run as stored: its header and one record per step.

use rill_types::{ObjectHash, Record, Timestamp};
use serde::{Deserialize, Serialize};

use crate::remote::BlobDigest;
use crate::verify::{StepResult, StepStatus};

/// Where a step is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepState {
    /// Queued for a worker.
    Queued,
    /// A worker is running it.
    Running,
    /// Done, as it ended.
    Done(StepEnd),
}

/// How a step ended; [`StepStatus`] as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepEnd {
    Passed,
    Failed { exit_code: i32 },
    TimedOut,
    Error(String),
    Unaffected,
    NotGating,
}

impl From<&StepStatus> for StepEnd {
    fn from(status: &StepStatus) -> Self {
        match status {
            StepStatus::Passed => StepEnd::Passed,
            StepStatus::Failed { exit_code } => StepEnd::Failed {
                exit_code: *exit_code,
            },
            StepStatus::TimedOut => StepEnd::TimedOut,
            StepStatus::Error(message) => StepEnd::Error(message.clone()),
            StepStatus::Unaffected => StepEnd::Unaffected,
            StepStatus::NotGating => StepEnd::NotGating,
        }
    }
}

/// A stored digest: hash and size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredDigest {
    pub hash: ObjectHash,
    pub size: u64,
}

impl From<BlobDigest> for StoredDigest {
    fn from(d: BlobDigest) -> Self {
        StoredDigest {
            hash: d.hash,
            size: d.size,
        }
    }
}

impl From<StoredDigest> for BlobDigest {
    fn from(d: StoredDigest) -> Self {
        BlobDigest {
            hash: d.hash,
            size: d.size,
        }
    }
}

/// One step (or shard) of a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepRecord {
    pub step: String,
    pub shard: Option<(u32, u32)>,
    pub state: StepState,
    pub cached: bool,
    pub action: Option<StoredDigest>,
    pub stdout: Option<StoredDigest>,
    pub stderr: Option<StoredDigest>,
    /// Output files: the artifacts.
    pub outputs: Vec<(String, StoredDigest)>,
    pub worker: Option<String>,
    pub duration_ms: Option<u64>,
    /// It ran uncached because a probe found it non-hermetic.
    pub nonhermetic: bool,
    pub updated_at: Timestamp,
}

impl StepRecord {
    /// A step waiting in a queue or on a worker.
    pub fn pending(
        step: &str,
        shard: Option<(u32, u32)>,
        action: BlobDigest,
        running: bool,
    ) -> Self {
        StepRecord {
            step: step.to_string(),
            shard,
            state: if running {
                StepState::Running
            } else {
                StepState::Queued
            },
            cached: false,
            action: Some(action.into()),
            stdout: None,
            stderr: None,
            outputs: Vec::new(),
            worker: None,
            duration_ms: None,
            nonhermetic: false,
            updated_at: Timestamp::now(),
        }
    }

    /// The record of a finished step.
    pub fn finished(result: &StepResult) -> Self {
        StepRecord {
            step: result.step.clone(),
            shard: result.shard,
            state: StepState::Done((&result.status).into()),
            cached: result.cached,
            action: result.action.map(Into::into),
            stdout: result.stdout.map(Into::into),
            stderr: result.stderr.map(Into::into),
            outputs: result
                .outputs
                .iter()
                .map(|(path, digest)| (path.clone(), (*digest).into()))
                .collect(),
            worker: result.worker.clone(),
            duration_ms: result.duration.map(|d| d.as_millis() as u64),
            nonhermetic: result.nonhermetic,
            updated_at: Timestamp::now(),
        }
    }
}

/// How a run ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunEnd {
    pub green: bool,
    pub summary: String,
    pub finished_at: Timestamp,
}

/// A run's header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunHeader {
    pub id: String,
    pub version: ObjectHash,
    /// Verification of trunk, after landing.
    pub trunk: bool,
    pub started_at: Timestamp,
    /// `None` while it runs.
    pub end: Option<RunEnd>,
}

impl Record for RunHeader {
    const TAG: &'static str = "verify-run";
}

/// A step as stored, with its index in the run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexedStep {
    pub index: u32,
    pub step: StepRecord,
}

impl Record for IndexedStep {
    const TAG: &'static str = "verify-step";
}

/// A run with its steps, by index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub header: RunHeader,
    pub steps: Vec<IndexedStep>,
}

impl Run {
    /// Step `index`.
    pub fn step(&self, index: u32) -> Option<&StepRecord> {
        self.steps
            .iter()
            .find(|s| s.index == index)
            .map(|s| &s.step)
    }
}
