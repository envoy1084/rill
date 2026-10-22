//! Finding repositories other servers created and starting their workers.

use std::sync::Arc;

/// How often a server looks for repositories other servers created.
const DISCOVER_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

/// Build the context (and so start the workers) of every repository,
/// including ones other servers create later, until `shutdown`: any server
/// may be the one left to land or index for them.
pub async fn discover_repos(
    state: Arc<crate::state::AppState>,
    shutdown: tokio_util::sync::CancellationToken,
) {
    let mut tick = tokio::time::interval(DISCOVER_INTERVAL);
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => return,
            _ = tick.tick() => {}
        }
        if let Err(e) = state.repos.warm().await {
            tracing::warn!(error = %e, "discovering repositories failed");
        }
    }
}
