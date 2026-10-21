//! Index jobs: keeping the search index current with trunk.

use std::sync::Arc;

use async_trait::async_trait;
use rill_jobs::{Claim, Handler, Job, JobFailure, Queue, Worker};
use rill_lander::land::followups::{index_job, INDEX_QUEUE};
use rill_types::ChangeVersionHash;
use tokio_util::sync::CancellationToken;

use crate::state::RepoContext;

struct IndexHandler {
    ctx: Arc<RepoContext>,
}

#[async_trait]
impl Handler for IndexHandler {
    async fn run(&self, job: &Job, _claim: &Claim) -> Result<(), JobFailure> {
        let version = ChangeVersionHash::from_slice(&job.payload)
            .map_err(|e| JobFailure::Fatal(format!("bad payload: {e}")))?;
        let retry = |e: rill_search::Error| JobFailure::Retry(e.to_string());
        let (root, stats) = self
            .ctx
            .search
            .index_version(&version)
            .await
            .map_err(retry)?;
        self.ctx.search.flush().await.map_err(retry)?;
        tracing::debug!(%version, %root, files = stats.files, "indexed trunk version");
        Ok(())
    }
}

/// Run index jobs until `shutdown`. Trunk can move without a landing job
/// (imports), so the current head is queued once at start; indexing is
/// content-addressed, so an already indexed head costs nothing.
pub async fn run(ctx: Arc<RepoContext>, shutdown: CancellationToken) {
    let queue = Queue::new(ctx.repo.clone(), INDEX_QUEUE);
    match ctx.repo.trunk_head().await {
        Ok(Some((_, head))) => {
            if let Err(e) = queue.push(index_job(&head)).await {
                tracing::warn!(error = %e, "queueing the trunk head for indexing failed");
            }
        }
        Ok(None) => {}
        Err(e) => tracing::warn!(error = %e, "reading the trunk head failed"),
    }
    let handler = Arc::new(IndexHandler { ctx: ctx.clone() });
    Worker::new(queue, handler).run(shutdown).await;
    if let Err(e) = ctx.search.flush().await {
        tracing::warn!(error = %e, "flushing search index at shutdown failed");
    }
    tracing::info!(repo = %ctx.config.name, "index worker stopped");
}
