//! Automatic reverts, scheduled as jobs: the grace period before a revert
//! is a job's ready time, so a restart does not forget a pending revert,
//! and any worker may carry it out.

use std::sync::Arc;

use async_trait::async_trait;
use rill_jobs::{Claim, Handler, Job, JobFailure, JobSpec, Priority, Queue};
use rill_types::{Timestamp, TrunkSeq};

use crate::error::Result;
use crate::land::Lander;
use crate::revert::auto_revert;

/// Queue of pending automatic reverts.
pub const REVERT_QUEUE: &str = "revert";
/// Job kind on [`REVERT_QUEUE`].
pub const REVERT_JOB: &str = "auto_revert";

/// Revert `culprit` once the lander's `revert_grace` has passed, unless a
/// forward fix lands first.
pub async fn schedule_auto_revert(lander: &Lander, culprit: TrunkSeq) -> Result<()> {
    let grace = lander.policy().revert_grace.as_millis() as i64;
    let run_at = Timestamp::from_millis(Timestamp::now().millis() + grace);
    let spec = JobSpec::new(
        REVERT_JOB,
        culprit.get().to_string(),
        culprit.get().to_be_bytes().to_vec(),
    )
    .priority(Priority::High);
    Queue::new(lander.repo().clone(), REVERT_QUEUE)
        .push_at(spec, run_at)
        .await?;
    tracing::info!(%culprit, grace_ms = grace, "automatic revert scheduled");
    Ok(())
}

/// Carries out scheduled reverts.
pub struct RevertHandler {
    lander: Arc<Lander>,
}

impl RevertHandler {
    pub fn new(lander: Arc<Lander>) -> Arc<Self> {
        Arc::new(RevertHandler { lander })
    }
}

#[async_trait]
impl Handler for RevertHandler {
    async fn run(&self, job: &Job, _claim: &Claim) -> std::result::Result<(), JobFailure> {
        let seq: [u8; 8] = job
            .payload
            .as_slice()
            .try_into()
            .map_err(|_| JobFailure::Fatal("bad payload".into()))?;
        let culprit = TrunkSeq(u64::from_be_bytes(seq));
        match auto_revert(&self.lander, culprit).await {
            Ok(Some(outcome)) => {
                tracing::warn!(%culprit, revert = %outcome.seq, "auto-reverted culprit");
                Ok(())
            }
            Ok(None) => {
                tracing::info!(%culprit, "forward fix landed; no auto-revert");
                Ok(())
            }
            Err(e) => Err(JobFailure::Retry(e.to_string())),
        }
    }
}
