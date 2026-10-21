//! Sampling cluster-wide state into gauges: queue depths, trunk, events
//! and lease epochs. Every server reports the same values from the shared
//! store, so dashboards take the maximum across servers.

use std::sync::Arc;
use std::time::Duration;

use rill_jobs::Queue;
use rill_lander::land::followups::{DERIVE_QUEUE, INDEX_QUEUE};
use tokio_util::sync::CancellationToken;

use super::metrics::{QueueLabels, RepoLabels, RoleLabels};
use crate::state::{AppState, RepoContext};

/// How often gauges are refreshed.
const INTERVAL: Duration = Duration::from_secs(15);
/// The lease roles a repository has.
const ROLES: [&str; 3] = [
    rill_lander::land::LANDER_LEASE,
    rill_lander::postland::POSTLAND_LEASE,
    crate::workers::SEARCH_LEASE,
];

/// Refresh the gauges of every loaded repository until `shutdown`.
pub async fn sample_forever(state: Arc<AppState>, shutdown: CancellationToken) {
    let mut tick = tokio::time::interval(INTERVAL);
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => return,
            _ = tick.tick() => {}
        }
        for ctx in state.repos.loaded() {
            if let Err(e) = sample(&state, &ctx).await {
                tracing::debug!(repo = %ctx.config.name, error = %e, "sampling metrics failed");
            }
        }
    }
}

async fn sample(state: &AppState, ctx: &RepoContext) -> crate::error::Result<()> {
    let metrics = &state.metrics;
    let repo = format!("{}/{}", ctx.config.org, ctx.config.name);
    for queue in [DERIVE_QUEUE, INDEX_QUEUE] {
        let stats = Queue::new(ctx.repo.clone(), queue).stats().await?;
        let labels = QueueLabels {
            repo: repo.clone(),
            queue: queue.to_string(),
        };
        metrics
            .jobs_pending
            .get_or_create(&labels)
            .set(stats.pending());
        metrics.jobs_dead.get_or_create(&labels).set(stats.dead);
        metrics
            .jobs_retried
            .get_or_create(&labels)
            .set(stats.retried);
    }
    let labels = RepoLabels { repo: repo.clone() };
    let pending = ctx.repo.pending_land_requests(10_000).await?.len();
    metrics
        .land_requests_pending
        .get_or_create(&labels)
        .set(pending as i64);
    let head = ctx
        .repo
        .trunk_head()
        .await?
        .map(|(s, _)| s.get())
        .unwrap_or(0);
    metrics.trunk_head.get_or_create(&labels).set(head as i64);
    let events = ctx.repo.event_count().await?;
    metrics.events_written.get_or_create(&labels).set(events);
    for role in ROLES {
        let epoch = ctx.repo.lease_epoch(role).await?;
        metrics
            .lease_epoch
            .get_or_create(&RoleLabels {
                repo: repo.clone(),
                role: role.to_string(),
            })
            .set(epoch as i64);
    }
    Ok(())
}
