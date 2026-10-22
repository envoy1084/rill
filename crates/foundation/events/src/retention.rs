//! Dropping old events.

use std::time::Duration;

use rill_store::RepoStore;
use rill_types::Timestamp;
use tokio_util::sync::CancellationToken;

use crate::error::Result;

/// How long events are kept.
pub const RETENTION: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// Events deleted per transaction.
const BATCH: usize = 1_000;

/// Delete every event of `repo` older than `before`; returns how many.
pub async fn sweep(repo: &RepoStore, before: Timestamp) -> Result<usize> {
    let mut total = 0;
    loop {
        let removed = repo.sweep_events(before, BATCH).await?;
        total += removed;
        if removed < BATCH {
            return Ok(total);
        }
    }
}

/// Sweep events past [`RETENTION`] every `interval` until `shutdown`.
pub async fn run_retention(repo: RepoStore, interval: Duration, shutdown: CancellationToken) {
    let mut tick = tokio::time::interval(interval);
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => return,
            _ = tick.tick() => {}
        }
        let before =
            Timestamp::from_millis(Timestamp::now().millis() - RETENTION.as_millis() as i64);
        match sweep(&repo, before).await {
            Ok(0) => {}
            Ok(n) => tracing::debug!(removed = n, "swept expired events"),
            Err(e) => tracing::warn!(error = %e, "sweeping events failed"),
        }
    }
}
