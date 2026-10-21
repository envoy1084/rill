//! Keeps background derived data current with trunk.

use std::sync::Arc;
use std::time::Duration;

use rill_core::derive::BackgroundDeriver;
use rill_lander::LandEvent;
use tokio::sync::broadcast::error::RecvError;
use tokio_util::sync::CancellationToken;

use crate::state::RepoContext;

/// Trunk versions derived per pass, so one pass never holds the worker
/// for long.
const PASS_LIMIT: usize = 64;
/// Passes also run on this interval, to retry failures and catch up after
/// a lagged event stream.
const INTERVAL: Duration = Duration::from_secs(5);

/// Derive background kinds for every trunk version, starting after the
/// stored cursor, until `shutdown` fires.
pub async fn run(ctx: Arc<RepoContext>, shutdown: CancellationToken) {
    let deriver = BackgroundDeriver::new(ctx.derive.clone());
    let mut rx = ctx.lander.subscribe();
    let mut tick = tokio::time::interval(INTERVAL);
    loop {
        catch_up(&ctx, &deriver).await;
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = tick.tick() => {}
            event = rx.recv() => match event {
                Ok(LandEvent::Landed { .. }) | Err(RecvError::Lagged(_)) => {}
                Ok(_) => continue,
                Err(RecvError::Closed) => break,
            },
        }
    }
    tracing::info!(repo = %ctx.config.name, "background deriver stopped");
}

async fn catch_up(ctx: &RepoContext, deriver: &BackgroundDeriver) {
    loop {
        match deriver.pass(PASS_LIMIT).await {
            Ok(pass) if pass.versions == PASS_LIMIT => continue,
            Ok(pass) => {
                if pass.versions > 0 {
                    tracing::debug!(repo = %ctx.config.name, versions = pass.versions, cursor = pass.cursor.get(), "derived trunk versions");
                }
                return;
            }
            Err(e) => {
                tracing::warn!(repo = %ctx.config.name, error = %e, "background derivation pass failed");
                return;
            }
        }
    }
}
