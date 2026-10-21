//! Background tasks that run per repository: the landing loop, post-land
//! verification, the derive and index job workers, event retention and
//! the follower that keeps cached policy and relations current.

mod deriver;
mod follower;
mod hooks;
mod indexer;
pub use indexer::SEARCH_LEASE;
mod leader;
mod reverter;
mod verifier;

use std::sync::Arc;

pub use hooks::ReviewHooksAdapter;
pub use verifier::ExecVerifier;

use crate::state::RepoContext;

/// How often expired events are swept.
const EVENT_SWEEP_INTERVAL: std::time::Duration = std::time::Duration::from_secs(3600);

/// Spawn the tasks `roles` call for over `ctx`. They run until the
/// context's shutdown token is cancelled. Every process follows shared
/// state; background work runs only in processes with the workers role.
pub fn spawn_repo_workers(ctx: &Arc<RepoContext>, roles: crate::config::Roles) {
    let shutdown = ctx.shutdown.clone();
    ctx.lifecycle
        .tasks
        .spawn(follower::run(ctx.clone(), shutdown.clone()));
    if !roles.workers {
        return;
    }

    ctx.lifecycle
        .tasks
        .spawn(ctx.lander.clone().run_forever(shutdown.clone()));
    if let Some(postland) = &ctx.postland {
        ctx.lifecycle.tasks.spawn(
            postland
                .clone()
                .run_forever(ctx.postland_interval(), shutdown.clone()),
        );
    }
    ctx.lifecycle
        .tasks
        .spawn(deriver::run(ctx.clone(), shutdown.clone()));
    ctx.lifecycle
        .tasks
        .spawn(reverter::run(ctx.clone(), shutdown.clone()));
    ctx.lifecycle.tasks.spawn(rill_events::run_retention(
        ctx.repo.clone(),
        EVENT_SWEEP_INTERVAL,
        shutdown.clone(),
    ));
    ctx.lifecycle
        .tasks
        .spawn(indexer::run(ctx.clone(), shutdown));
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
