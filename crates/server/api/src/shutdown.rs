//! Shutting down in order: stop being ready, let load balancers notice,
//! stop accepting and finish what is running, and past the deadline give
//! running jobs back.

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::serve::{serve_with_state, Listeners};
use rill_runtime::error::{Error, Result};
use rill_runtime::state::AppState;

/// How long aborted tasks get to give their jobs back.
const ABORT_WAIT: Duration = Duration::from_secs(2);

/// Serve until `signal`, then shut down as `[shutdown]` says.
pub async fn serve_until(
    state: Arc<AppState>,
    listeners: Listeners,
    signal: CancellationToken,
) -> Result<()> {
    let lifecycle = state.lifecycle.clone();
    let mut serving = tokio::spawn(serve_with_state(
        state.clone(),
        listeners,
        lifecycle.stop.clone(),
    ));
    tokio::select! {
        _ = signal.cancelled() => {}
        outcome = &mut serving => {
            return outcome.map_err(|e| Error::Transport(format!("server task: {e}")))?;
        }
    }
    let config = state.config.shutdown;
    lifecycle.start_draining();
    tracing::info!(
        drain_secs = config.drain_secs,
        grace_secs = config.grace_secs,
        "draining"
    );
    tokio::time::sleep(Duration::from_secs(config.drain_secs)).await;

    lifecycle.stop.cancel();
    lifecycle.tasks.close();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(config.grace_secs);
    let served = tokio::time::timeout_at(deadline, serving).await;
    let finished = tokio::time::timeout_at(deadline, lifecycle.tasks.wait())
        .await
        .is_ok();
    if served.is_err() || !finished {
        tracing::warn!("shutdown deadline passed; giving running jobs back");
        lifecycle.abort.cancel();
        let _ = tokio::time::timeout(ABORT_WAIT, lifecycle.tasks.wait()).await;
    }
    match served {
        Ok(outcome) => outcome.map_err(|e| Error::Transport(format!("server task: {e}")))?,
        Err(_) => Ok(()),
    }
}
