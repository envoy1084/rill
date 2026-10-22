//! Keeping the identity view current: apply the identity log as it
//! grows, and reload everything now and then and when a pending key
//! activates.

use std::sync::Arc;
use std::time::Duration;

use rill_types::Timestamp;
use tokio_util::sync::CancellationToken;

use super::view::IdentityView;

/// How long a follower waits for the log before checking anyway.
const LOG_WAIT: Duration = Duration::from_secs(5);
/// How often the whole view is reread, covering anything a follower
/// missed.
const FULL_RELOAD: Duration = Duration::from_secs(60);
/// Entries applied per read.
const BATCH: usize = 1000;

/// Follow the identity log into `view` until `shutdown`.
pub async fn follow(view: Arc<IdentityView>, shutdown: CancellationToken) {
    let mut cursor = None;
    let mut seen = 0;
    let mut last_full = tokio::time::Instant::now();
    let mut started = false;
    loop {
        if !started {
            match start(&view).await {
                Ok((c, n)) => {
                    (cursor, seen) = (c, n);
                    started = true;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "starting the identity follower failed");
                    if pause(&shutdown, Duration::from_secs(1)).await {
                        return;
                    }
                    continue;
                }
            }
        }
        let wait = view.store().wait_for_log(seen, LOG_WAIT);
        let count = tokio::select! {
            _ = shutdown.cancelled() => return,
            count = wait => count,
        };
        match count {
            Ok(n) => seen = n,
            Err(e) => {
                tracing::warn!(error = %e, "waiting on the identity log failed");
                if pause(&shutdown, Duration::from_secs(1)).await {
                    return;
                }
                continue;
            }
        }
        match catch_up(&view, cursor).await {
            Ok(next) => cursor = next,
            Err(e) => tracing::warn!(error = %e, "reading the identity log failed"),
        }
        let activation_due = view
            .next_activation()
            .is_some_and(|t| t <= Timestamp::now());
        if activation_due || last_full.elapsed() >= FULL_RELOAD {
            match view.reload().await {
                Ok(()) => last_full = tokio::time::Instant::now(),
                Err(e) => tracing::warn!(error = %e, "reloading identity failed"),
            }
        }
    }
}

/// The log position to follow from: the end, after a full reload, so
/// nothing between the two is missed.
async fn start(
    view: &IdentityView,
) -> crate::error::Result<(Option<rill_types::Versionstamp>, i64)> {
    let seen = view.store().log_count().await?;
    let cursor = view.store().latest_log_entry().await?;
    view.reload().await?;
    Ok((cursor, seen))
}

async fn catch_up(
    view: &IdentityView,
    mut cursor: Option<rill_types::Versionstamp>,
) -> crate::error::Result<Option<rill_types::Versionstamp>> {
    loop {
        let entries = view.store().log_after(cursor, BATCH).await?;
        let mut keys_changed = false;
        for (stamp, event) in &entries {
            keys_changed |= view.apply(&event.change);
            cursor = Some(*stamp);
        }
        if keys_changed {
            view.reload_keys().await?;
        }
        if entries.len() < BATCH {
            return Ok(cursor);
        }
    }
}

/// Sleep `d` unless shut down first; returns whether it was shut down.
async fn pause(shutdown: &CancellationToken, d: Duration) -> bool {
    tokio::select! {
        _ = shutdown.cancelled() => true,
        _ = tokio::time::sleep(d) => false,
    }
}
