//! Derive jobs: background data for each landed version, ahead of reads.

use std::sync::Arc;

use async_trait::async_trait;
use rill_jobs::{Claim, Handler, Job, JobFailure, Queue, Worker};
use rill_lander::land::followups::DERIVE_QUEUE;
use rill_types::ChangeVersionHash;
use tokio_util::sync::CancellationToken;

use crate::state::RepoContext;

/// Derive jobs run at a time per repository.
const CONCURRENCY: usize = 2;

struct DeriveHandler {
    ctx: Arc<RepoContext>,
}

#[async_trait]
impl Handler for DeriveHandler {
    async fn run(&self, job: &Job, _claim: &Claim) -> Result<(), JobFailure> {
        let version = ChangeVersionHash::from_slice(&job.payload)
            .map_err(|e| JobFailure::Fatal(format!("bad payload: {e}")))?;
        self.ctx
            .derive
            .ensure_background(&version)
            .await
            .map_err(|e| JobFailure::Retry(e.to_string()))?;
        tracing::debug!(repo = %self.ctx.config.name, %version, "derived background kinds");
        Ok(())
    }
}

/// Run derive jobs until `shutdown`.
pub async fn run(ctx: Arc<RepoContext>, shutdown: CancellationToken) {
    let queue = Queue::new(ctx.repo.clone(), DERIVE_QUEUE);
    let handler = Arc::new(DeriveHandler { ctx: ctx.clone() });
    Worker::new(queue, handler)
        .concurrency(CONCURRENCY)
        .abort_on(ctx.lifecycle.abort.clone())
        .run(shutdown)
        .await;
    tracing::info!(repo = %ctx.config.name, "derive worker stopped");
}
