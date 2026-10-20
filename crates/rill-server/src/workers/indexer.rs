//! Keeps the search index current with trunk.

use std::sync::Arc;

use rill_lander::LandEvent;
use tokio::sync::broadcast::error::RecvError;
use tokio_util::sync::CancellationToken;

use crate::state::RepoContext;

/// Index the current trunk head, then every version that lands, until
/// `shutdown` fires. Indexing is content-addressed, so re-indexing a root
/// that was already indexed is a no-op.
pub async fn run(ctx: Arc<RepoContext>, shutdown: CancellationToken) {
    let mut rx = ctx.lander.subscribe();
    if let Ok(Some((_, head))) = ctx.repo.trunk_head().await {
        index(&ctx, head).await;
    }
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            event = rx.recv() => match event {
                Ok(LandEvent::Landed { version, .. }) => index(&ctx, version).await,
                Ok(_) => {}
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "search indexer lagged; re-indexing head");
                    if let Ok(Some((_, head))) = ctx.repo.trunk_head().await {
                        index(&ctx, head).await;
                    }
                }
                Err(RecvError::Closed) => break,
            },
        }
    }
    if let Err(e) = ctx.search.flush().await {
        tracing::warn!(error = %e, "flushing search index at shutdown failed");
    }
    tracing::info!(repo = %ctx.config.name, "search indexer stopped");
}

async fn index(ctx: &RepoContext, version: rill_types::ChangeVersionHash) {
    match ctx.search.index_version(&version).await {
        Ok((root, stats)) => {
            tracing::debug!(%version, %root, files = stats.files, "indexed landed version");
            if let Err(e) = ctx.search.flush().await {
                tracing::warn!(error = %e, "flushing search index failed");
            }
        }
        Err(e) => tracing::warn!(%version, error = %e, "indexing landed version failed"),
    }
}
