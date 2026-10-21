//! Background tasks that run per repository: the landing loop, post-land
//! verification, background derivation, search indexing and the event
//! forwarders that feed the repository's event bus.

mod deriver;
mod hooks;
mod indexer;
mod verifier;

use std::sync::Arc;

pub use hooks::ReviewHooksAdapter;
pub use verifier::ExecVerifier;

use crate::state::{land_to_bus, RepoContext, RepoEvent};

/// Spawn every worker for `ctx`. They run until the context's shutdown
/// token is cancelled; the forwarders also stop when their source channel
/// closes.
pub fn spawn_repo_workers(ctx: &Arc<RepoContext>) {
    let shutdown = ctx.shutdown.clone();
    ctx.events
        .forward(ctx.review().subscribe(), |e| vec![RepoEvent::Review(e)]);
    ctx.events.forward(ctx.lander.subscribe(), land_to_bus);

    tokio::spawn(ctx.lander.clone().run_forever(shutdown.clone()));
    if let Some(postland) = &ctx.postland {
        tokio::spawn(
            postland
                .clone()
                .run_forever(ctx.postland_interval(), shutdown.clone()),
        );
    }
    tokio::spawn(deriver::run(ctx.clone(), shutdown.clone()));
    tokio::spawn(indexer::run(ctx.clone(), shutdown));
    tracing::info!(repo = %ctx.config.name, "repository workers started");
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
