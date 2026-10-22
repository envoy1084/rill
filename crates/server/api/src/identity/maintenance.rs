//! Key rotation and sweeping expired identity records. Every process with
//! the workers role runs it; each step is safe to run concurrently (a
//! rotation race leaves one new key, sweeps delete what is expired).

use std::sync::Arc;
use std::time::Duration;

use rill_types::Timestamp;
use tokio_util::sync::CancellationToken;

use super::view::{system_actor, IdentityView};

/// How often maintenance runs.
const INTERVAL: Duration = Duration::from_secs(600);
/// How long identity log entries are kept.
const LOG_RETENTION_MS: i64 = 7 * 86_400_000;
/// How long expired sessions and revocations are kept.
const EXPIRED_RETENTION_MS: i64 = 86_400_000;
/// Records swept per run and table.
const SWEEP_LIMIT: usize = 10_000;

/// Maintain identity records until `shutdown`.
pub async fn maintain_forever(view: Arc<IdentityView>, shutdown: CancellationToken) {
    let mut tick = tokio::time::interval(INTERVAL);
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => return,
            _ = tick.tick() => {}
        }
        if let Err(e) = maintain(&view).await {
            tracing::warn!(error = %e, "identity maintenance failed");
        }
    }
}

async fn maintain(view: &IdentityView) -> crate::error::Result<()> {
    let now = Timestamp::now();
    if view.keys().maintain(now, &system_actor()).await? {
        tracing::info!("token signing key rotated");
    }
    let expired = Timestamp::from_millis(now.millis() - EXPIRED_RETENTION_MS);
    let store = view.store();
    store.sweep_revocations(expired, SWEEP_LIMIT).await?;
    store.sweep_sessions(expired, SWEEP_LIMIT).await?;
    store.sweep_pending_logins(now, SWEEP_LIMIT).await?;
    store
        .sweep_log(
            Timestamp::from_millis(now.millis() - LOG_RETENTION_MS),
            SWEEP_LIMIT,
        )
        .await?;
    Ok(())
}
