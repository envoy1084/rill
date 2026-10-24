//! Index jobs: keeping the search index current with trunk. One server
//! writes the index at a time, holding the repository's `search` lease;
//! the others refresh their copies from what it flushed.

use std::sync::Arc;

use async_trait::async_trait;
use rill_jobs::{Claim, Handler, Job, JobFailure, Queue, Worker};
use rill_lander::land::followups::{index_job, INDEX_JOB, INDEX_QUEUE, INDEX_ROOT_JOB};
use rill_store::repo::Lease;
use rill_types::{ChangeVersionHash, TreeHash};
use tokio_util::sync::CancellationToken;

use super::leader::while_leader;
use crate::state::RepoContext;

/// The lease role of the search index writer.
pub const SEARCH_LEASE: &str = "search";

struct IndexHandler {
    ctx: Arc<RepoContext>,
    lease: Lease,
}

#[async_trait]
impl Handler for IndexHandler {
    async fn run(&self, job: &Job, _claim: &Claim) -> Result<(), JobFailure> {
        let bad = |e: rill_types::Error| JobFailure::Fatal(format!("bad payload: {e}"));
        let retry = |e: rill_search::Error| JobFailure::Retry(e.to_string());
        let search = &self.ctx.search;
        let fence = Some(&self.lease);
        let (root, stats) = match job.kind.as_str() {
            INDEX_JOB => {
                let version = ChangeVersionHash::from_slice(&job.payload).map_err(bad)?;
                search.index_version(&version, fence).await.map_err(retry)?
            }
            INDEX_ROOT_JOB => {
                let root = TreeHash::from_slice(&job.payload).map_err(bad)?;
                (root, search.index_root(root, fence).await.map_err(retry)?)
            }
            other => return Err(JobFailure::Fatal(format!("unknown job kind {other:?}"))),
        };
        tracing::debug!(%root, files = stats.files, "indexed root");
        // Merging is a no-op until a tier fills; doing it here keeps one
        // writer for the segment list.
        let compacted = search.compact(fence).await.map_err(retry)?;
        if compacted.merges > 0 {
            tracing::debug!(
                merges = compacted.merges,
                segments = compacted.segments_after,
                "compacted the search index"
            );
        }
        Ok(())
    }
}

/// Run index jobs while this server holds the search lease, until
/// `shutdown`. Trunk can move without a landing job (imports), so the
/// current head is queued once at start; indexing is content-addressed,
/// so an already indexed head costs nothing.
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
    let repo = ctx.repo.clone();
    while_leader(repo, SEARCH_LEASE, shutdown, |lease, token| {
        let (ctx, queue) = (ctx.clone(), queue.clone());
        async move {
            // Another server may have written since this one last led.
            if let Err(e) = ctx.search.load().await {
                tracing::warn!(error = %e, "loading the search index failed");
                return;
            }
            let abort = ctx.lifecycle.abort.clone();
            let handler = Arc::new(IndexHandler { ctx, lease });
            Worker::new(queue, handler).abort_on(abort).run(token).await;
        }
    })
    .await;
    tracing::info!(repo = %ctx.config.name, "index worker stopped");
}
