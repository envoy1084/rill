//! The record of reviewer runs on a change: queued, done, failed, stale
//! or skipped, one entry per (kind, agent, version).

use bytes::Bytes;
use rill_types::{ChangeId, ChangeVersionHash, Timestamp};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::lifecycle::MAX_WRITE_ATTEMPTS;
use crate::service::ReviewService;

/// Runs kept per change; the oldest are dropped beyond it.
pub const MAX_RUNS: usize = 200;

/// Whether a run reviews or resolves conflicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunKind {
    Review,
    Resolve,
}

/// Where a run stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunStatus {
    Queued,
    Done,
    Failed,
    /// The change had a new version before the result came.
    Stale,
    /// Not run: over budget, or the agent cannot review.
    Skipped,
}

/// One run of one agent on one version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewRun {
    pub kind: RunKind,
    /// The agent's name.
    pub agent: String,
    pub version: ChangeVersionHash,
    pub reason: String,
    pub status: RunStatus,
    pub queued_at: Timestamp,
    pub finished_at: Option<Timestamp>,
    /// Findings posted.
    pub findings: u32,
    /// The vote cast; 0 for none.
    pub vote: i8,
    pub model: String,
    /// The summary, or why it failed or was skipped.
    pub detail: String,
}

impl ReviewRun {
    /// A run just queued.
    pub fn queued(
        kind: RunKind,
        agent: &str,
        version: ChangeVersionHash,
        reason: impl Into<String>,
    ) -> Self {
        ReviewRun {
            kind,
            agent: agent.to_string(),
            version,
            reason: reason.into(),
            status: RunStatus::Queued,
            queued_at: Timestamp::now(),
            finished_at: None,
            findings: 0,
            vote: 0,
            model: String::new(),
            detail: String::new(),
        }
    }

    /// Whether this is `agent`'s `kind` run on `version`.
    pub fn is(&self, kind: RunKind, agent: &str, version: &ChangeVersionHash) -> bool {
        self.kind == kind && self.agent == agent && self.version == *version
    }

    /// Mark the run finished with `status`.
    pub fn finish(&mut self, status: RunStatus, detail: impl Into<String>) {
        self.status = status;
        self.detail = detail.into();
        self.finished_at = Some(Timestamp::now());
    }
}

fn runs_key(change: &ChangeId) -> String {
    format!("review-runs:{change}")
}

fn decode(bytes: Option<&Bytes>) -> Result<Vec<ReviewRun>> {
    match bytes {
        Some(b) => serde_json::from_slice(b)
            .map_err(|e| Error::Store(rill_store::Error::Codec(format!("review runs: {e}")))),
        None => Ok(Vec::new()),
    }
}

impl ReviewService {
    /// The change's runs, oldest first.
    pub async fn review_runs(&self, change: &ChangeId) -> Result<Vec<ReviewRun>> {
        decode(self.repo.get_misc(&runs_key(change)).await?.as_ref())
    }

    /// Change the change's runs with `f`, retrying when another write came
    /// first; returns what `f` returned on the attempt that was written.
    pub async fn update_review_runs<T>(
        &self,
        change: &ChangeId,
        mut f: impl FnMut(&mut Vec<ReviewRun>) -> T,
    ) -> Result<T> {
        let key = runs_key(change);
        for _ in 0..MAX_WRITE_ATTEMPTS {
            let current = self.repo.get_misc(&key).await?;
            let mut runs = decode(current.as_ref())?;
            let out = f(&mut runs);
            if runs.len() > MAX_RUNS {
                runs.drain(..runs.len() - MAX_RUNS);
            }
            let value = serde_json::to_vec(&runs)
                .map_err(|e| Error::Store(rill_store::Error::Codec(e.to_string())))?;
            match self.repo.set_misc_cas(&key, current, value.into()).await {
                Ok(()) => return Ok(out),
                Err(e) if e.is_conflict() => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Err(Error::Contended(*change))
    }
}
