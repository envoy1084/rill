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
