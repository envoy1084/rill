//! Starting the tasks one repository runs, by process role.

use std::sync::Arc;

use super::{deriver, follower, indexer, reverter};
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
    ctx.lifecycle.tasks.spawn(
        ctx.releases
            .clone()
            .run_forever(ctx.lander.policy().batch_window, shutdown.clone()),
    );
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
