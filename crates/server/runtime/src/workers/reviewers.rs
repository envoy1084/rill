//! Routing changes to reviewer and resolver agents.
//!
//! When a change enters review, or gets a version whose content differs
//! from the last (not a message edit or trivial rebase, whose votes are
//! copied), every reviewer trunk's `.rill/review.toml` routes it to gets a
//! job. A version with conflicts on paths the configured resolver covers
//! gets a resolve job instead of reviews.

use std::sync::Arc;

use async_trait::async_trait;
use rill_events::EventFilter;
use rill_review::VersionRelation;
use rill_types::{ChangeId, ChangeVersionHash, Event, EventKind, ReviewStatus};

use super::consumer::Consumer;
use crate::error::Result;
use crate::fleet::{enqueue_resolve, enqueue_review};
use crate::state::RepoContext;

/// Routes changes to the fleet.
pub struct Router;

#[async_trait]
impl Consumer for Router {
    fn name(&self) -> &'static str {
        "review-router"
    }

    fn filter(&self) -> EventFilter {
        EventFilter::kinds(["review_submitted", "version_stored"])
    }

    async fn handle(&self, ctx: &Arc<RepoContext>, event: &Event) -> Result<()> {
        let (change, reason) = match &event.kind {
            EventKind::ReviewSubmitted { change, .. } => (*change, "submitted"),
            EventKind::VersionStored { change, .. } => (*change, "new version"),
            _ => return Ok(()),
        };
        route(ctx, change, reason, event).await
    }
}

async fn route(
    ctx: &Arc<RepoContext>,
    change: ChangeId,
    reason: &str,
    event: &Event,
) -> Result<()> {
    let review = ctx.review();
    let Some(state) = ctx.repo.get_review(&change).await? else {
        return Ok(());
    };
    if !matches!(state.status, ReviewStatus::Open | ReviewStatus::Landable) {
        return Ok(());
    }
    let Some(version) = state.current_version() else {
        return Ok(());
    };
    if let EventKind::VersionStored {
        version: stored, ..
    } = &event.kind
    {
        if *stored != version || !changed_content(ctx, &state.versions, version).await? {
            return Ok(());
        }
    }
    let config = review.trunk_review_config().await?;
    let facts = review.change_facts(&state).await?;
    let conflicted: Vec<_> = facts
        .paths
        .iter()
        .filter(|p| p.conflict)
        .map(|p| p.path.as_path())
        .collect();
    if !conflicted.is_empty() {
        if let Some(resolver) = config.resolver_for(&conflicted) {
            enqueue_resolve(ctx, &resolver.agent, change, version).await?;
        }
        return Ok(());
    }
    let paths: Vec<_> = facts.paths.iter().map(|p| p.path.as_path()).collect();
    for route in config.route(&paths, state.risk) {
        enqueue_review(ctx, &route.agent, change, version, reason).await?;
    }
    Ok(())
}

/// Whether `version` changed content against the version before it; the
/// first version is the submission's to route.
async fn changed_content(
    ctx: &RepoContext,
    versions: &[ChangeVersionHash],
    version: ChangeVersionHash,
) -> Result<bool> {
    let Some(i) = versions.iter().position(|v| *v == version) else {
        return Ok(false);
    };
    let Some(previous) = i.checked_sub(1).map(|j| versions[j]) else {
        return Ok(false);
    };
    let relation = ctx.review().relate_versions(&previous, &version).await?;
    Ok(relation == VersionRelation::Changed)
}
