//! Writing a verification's progress to its run, when runs are kept.
//! A failure to write is logged, never a reason to fail verification.

use std::sync::atomic::{AtomicU32, Ordering};

use super::outcome::StepResult;
use crate::runs::{RunHandle, StepRecord};

/// The run being written, and the next step index.
pub(crate) struct Progress {
    run: Option<RunHandle>,
    next: AtomicU32,
}

impl Progress {
    pub(crate) fn new(run: Option<RunHandle>) -> Self {
        Progress {
            run,
            next: AtomicU32::new(0),
        }
    }

    pub(crate) fn run_id(&self) -> Option<String> {
        self.run.as_ref().map(|r| r.id.clone())
    }

    /// A new step's index.
    pub(crate) fn index(&self) -> u32 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }

    pub(crate) async fn write(&self, index: u32, record: &StepRecord) {
        if let Some(run) = &self.run {
            if let Err(e) = run.step(index, record).await {
                tracing::warn!(run = %run.id, error = %e, "recording verification progress");
            }
        }
    }

    /// Record a step that has ended, under a new index.
    pub(crate) async fn ended(&self, result: &StepResult) {
        let index = self.index();
        self.write(index, &StepRecord::finished(result)).await;
    }

    pub(crate) async fn finish(&self, green: bool, summary: String) {
        if let Some(run) = &self.run {
            if let Err(e) = run.finish(green, summary).await {
                tracing::warn!(run = %run.id, error = %e, "recording the end of a verification");
            }
        }
    }
}
