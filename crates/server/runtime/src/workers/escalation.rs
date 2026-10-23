//! Escalating owner reviews that outlast their SLA.
//!
//! When a change enters review, or gets a new version, every owner set it
//! touches that lacks approvals and has an SLA gets a delayed job. When
//! the job runs and the set is still uncovered on that version, the
//! review is escalated one level (the owners' active delegates, then the
//! set's escalation contact, then the owners of the parent directory)
//! with a thread addressed to the target and a note the inbox reads, and
//! the next level is scheduled.

use std::sync::Arc;

use async_trait::async_trait;
use rill_events::EventFilter;
use rill_jobs::{Claim, Handler, Job, JobFailure, JobSpec, Queue, Worker};
use rill_policy::owners::{escalation_targets, OwnerSet};
use rill_review::lifecycle::service_actor;
use rill_types::{
    ChangeId, ChangeVersionHash, Event, EventKind, Principal, ReviewStatus, ThreadAnchor, Timestamp,
};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use super::consumer::Consumer;
use crate::error::{Error, Result};
use crate::state::RepoContext;

/// The queue escalation checks wait in.
pub const ESCALATE_QUEUE: &str = "escalate";
/// Prefix of the review notes recording escalations, by owner set id.
pub const ESCALATION_NOTE: &str = "escalation:";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Check {
    change: ChangeId,
    version: ChangeVersionHash,
    set: String,
    /// How many escalations happened before this check.
    level: u32,
}

/// Schedules checks as changes enter review or get new versions.
pub struct Scheduler;

#[async_trait]
impl Consumer for Scheduler {
    fn name(&self) -> &'static str {
        "escalation-scheduler"
    }

    fn filter(&self) -> EventFilter {
        EventFilter::kinds(["review_submitted", "version_stored"])
    }

    async fn handle(&self, ctx: &Arc<RepoContext>, event: &Event) -> Result<()> {
        let change = match &event.kind {
            EventKind::ReviewSubmitted { change, .. } | EventKind::VersionStored { change, .. } => {
                *change
            }
            _ => return Ok(()),
        };
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
        let queue = Queue::new(ctx.repo.clone(), ESCALATE_QUEUE);
        for cov in review.owner_coverage(&state).await? {
            let Some(sla) = cov.set.sla.filter(|_| !cov.covered) else {
                continue;
            };
            schedule(
                &queue,
                &Check {
                    change,
                    version,
                    set: cov.set.id.clone(),
                    level: 0,
                },
                sla,
            )
            .await?;
        }
        Ok(())
    }
}

async fn schedule(queue: &Queue, check: &Check, after: std::time::Duration) -> Result<()> {
    let key = format!(
        "{}:{}:{}:{}",
        check.change, check.version, check.set, check.level
    );
    let payload = serde_json::to_vec(check).map_err(|e| Error::internal(e.to_string()))?;
    let at = Timestamp::from_millis(Timestamp::now().millis() + after.as_millis() as i64);
    queue
        .push_at(JobSpec::new("escalate", key, payload), at)
        .await?;
    Ok(())
}

struct Escalator {
    ctx: Arc<RepoContext>,
}

#[async_trait]
impl Handler for Escalator {
    async fn run(&self, job: &Job, _claim: &Claim) -> std::result::Result<(), JobFailure> {
        let check: Check = serde_json::from_slice(&job.payload)
            .map_err(|e| JobFailure::Fatal(format!("bad payload: {e}")))?;
        escalate(&self.ctx, &check)
            .await
            .map_err(|e| JobFailure::Retry(e.to_string()))
    }
}

async fn escalate(ctx: &Arc<RepoContext>, check: &Check) -> Result<()> {
    let review = ctx.review();
    let Some(state) = ctx.repo.get_review(&check.change).await? else {
        return Ok(());
    };
    let still_waiting = matches!(state.status, ReviewStatus::Open | ReviewStatus::Landable)
        && state.current_version() == Some(check.version);
    if !still_waiting {
        return Ok(());
    }
    let Some(cov) = review
        .owner_coverage(&state)
        .await?
        .into_iter()
        .find(|c| c.set.id == check.set)
    else {
        return Ok(());
    };
    if cov.covered {
        return Ok(());
    }
    let now = Timestamp::now();
    let ladder = ladder(ctx, &cov.set, now).await?;
    let Some(target) = ladder.get(check.level as usize) else {
        return Ok(());
    };
    let to = principal_of(target);
    let body = format!(
        "The owner review for {} ({}) is past its SLA; escalated to {target}.",
        cov.set.id,
        cov.set.owners.join(", "),
    );
    review
        .comment(
            &check.change,
            &service_actor(),
            ThreadAnchor::Change,
            body,
            None,
            None,
            to,
        )
        .await?;
    review
        .set_note(
            &check.change,
            &service_actor(),
            &format!("{ESCALATION_NOTE}{}", cov.set.id),
            &format!("{}|{target}|{}", check.level + 1, now.millis()),
        )
        .await?;
    if let Some(sla) = cov.set.sla {
        let next = Check {
            level: check.level + 1,
            ..check.clone()
        };
        schedule(&Queue::new(ctx.repo.clone(), ESCALATE_QUEUE), &next, sla).await?;
    }
    Ok(())
}

/// Who an overdue set escalates to, in order: delegates and the
/// escalation contact, then the owners of the parent directory.
async fn ladder(ctx: &Arc<RepoContext>, set: &OwnerSet, now: Timestamp) -> Result<Vec<String>> {
    let mut out = escalation_targets(set, now);
    if !set.dir.is_root() {
        let owners = ctx.review().trunk_owners().await?;
        // A file in the parent directory stands for "the parent's owners".
        let probe = set
            .dir
            .parent()
            .to_owned_path()
            .join(rill_policy::owners::OWNERS_FILE)?;
        for parent in owners.sets_for(&probe) {
            out.extend(
                parent
                    .owners
                    .into_iter()
                    .filter(|o| !set.owners.contains(o)),
            );
        }
    }
    out.dedup();
    Ok(out)
}

fn principal_of(entry: &str) -> Option<Principal> {
    match entry.split_once(':')? {
        ("user", name) => Some(Principal::human(name)),
        ("agent", name) => Some(Principal::agent(name)),
        _ => None,
    }
}

/// Run escalation checks until `shutdown`.
pub async fn run(ctx: Arc<RepoContext>, shutdown: CancellationToken) {
    let queue = Queue::new(ctx.repo.clone(), ESCALATE_QUEUE);
    Worker::new(queue, Arc::new(Escalator { ctx: ctx.clone() }))
        .abort_on(ctx.lifecycle.abort.clone())
        .run(shutdown)
        .await;
}
