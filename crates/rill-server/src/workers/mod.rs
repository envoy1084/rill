//! Background tasks that run per repository: the landing loop, post-land
//! verification, the derive and index job workers, event retention and
//! the follower that keeps cached policy and relations current.

mod deriver;
mod follower;
mod hooks;
mod indexer;
mod leader;
mod verifier;

use std::sync::Arc;

pub use hooks::ReviewHooksAdapter;
pub use verifier::ExecVerifier;

use crate::state::RepoContext;

/// How often expired events are swept.
const EVENT_SWEEP_INTERVAL: std::time::Duration = std::time::Duration::from_secs(3600);

/// Spawn every worker for `ctx`. They run until the context's shutdown
/// token is cancelled.
pub fn spawn_repo_workers(ctx: &Arc<RepoContext>) {
    let shutdown = ctx.shutdown.clone();

    tokio::spawn(ctx.lander.clone().run_forever(shutdown.clone()));
    if let Some(postland) = &ctx.postland {
        tokio::spawn(
            postland
                .clone()
                .run_forever(ctx.postland_interval(), shutdown.clone()),
        );
    }
    tokio::spawn(deriver::run(ctx.clone(), shutdown.clone()));
    tokio::spawn(rill_events::run_retention(
        ctx.repo.clone(),
        EVENT_SWEEP_INTERVAL,
        shutdown.clone(),
    ));
    tokio::spawn(follower::run(ctx.clone(), shutdown.clone()));
    tokio::spawn(indexer::run(ctx.clone(), shutdown));
    tracing::info!(repo = %ctx.config.name, "repository workers started");
}

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

/// Delete expired idempotency records every ten minutes until `shutdown`.
pub async fn sweep_idempotency(
    stores: rill_store::Stores,
    shutdown: tokio_util::sync::CancellationToken,
) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(600));
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => return,
            _ = tick.tick() => {}
        }
        match stores.idem_sweep(10_000).await {
            Ok(0) => {}
            Ok(n) => tracing::debug!(removed = n, "swept expired idempotency records"),
            Err(e) => tracing::warn!(error = %e, "sweeping idempotency records failed"),
        }
    }
}
