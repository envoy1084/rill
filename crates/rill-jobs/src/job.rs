//! Jobs and their stored records.

use rill_types::{JobId, Record, Timestamp};
use serde::{Deserialize, Serialize};

/// How urgent a job is; higher priorities are claimed first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Priority {
    High,
    Normal,
    Low,
}

impl Priority {
    /// Every priority, most urgent first.
    pub const ALL: [Priority; 3] = [Priority::High, Priority::Normal, Priority::Low];

    pub(crate) fn rank(self) -> u8 {
        self as u8
    }
}

/// A job to enqueue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobSpec {
    /// What the handler should do.
    pub kind: String,
    /// What the job is about; picks its shard.
    pub key: String,
    pub payload: Vec<u8>,
    pub priority: Priority,
}

impl JobSpec {
    /// A normal-priority job.
    pub fn new(kind: impl Into<String>, key: impl Into<String>, payload: Vec<u8>) -> Self {
        JobSpec {
            kind: kind.into(),
            key: key.into(),
            payload,
            priority: Priority::Normal,
        }
    }

    /// The same job at `priority`.
    pub fn priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }
}

/// A stored job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub id: JobId,
    pub kind: String,
    pub key: String,
    pub payload: Vec<u8>,
    pub priority: Priority,
    /// Attempts that already ran out, by failing or by losing the lease.
    pub attempt: u32,
    pub enqueued_at: Timestamp,
    pub last_error: Option<String>,
}

impl Record for Job {
    const TAG: &'static str = "job";
}

/// A claimed job: who holds it, until when, and the token its fenced
/// writes check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Leased {
    pub job: Job,
    pub holder: String,
    pub token: [u8; 16],
    pub lease_until: Timestamp,
}

impl Record for Leased {
    const TAG: &'static str = "jobl";
}

/// A job that ran out of attempts or failed for good.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeadJob {
    pub job: Job,
    pub error: String,
    pub died_at: Timestamp,
}

impl Record for DeadJob {
    const TAG: &'static str = "jobd";
}
