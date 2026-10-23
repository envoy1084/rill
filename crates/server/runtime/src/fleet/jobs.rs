//! Queueing review and resolve jobs, with their run records and the
//! reviewers' daily budgets.

use std::time::Duration;

use bytes::Bytes;
use rill_jobs::{JobSpec, Queue, RetryPolicy};
use rill_policy::intents::day_of;
use rill_review::agents::{ReviewRun, RunKind, RunStatus};
use rill_types::{ChangeId, ChangeVersionHash, Principal, Timestamp};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::state::RepoContext;

/// The longest a lease request waits for a job.
pub const LEASE_WAIT_MAX: Duration = Duration::from_secs(30);

/// A reviewer's or resolver's lease: long enough for a model call, renewed
/// by the runner for longer ones.
const LEASE: Duration = Duration::from_secs(120);

/// The queue `agent`'s reviews wait in.
pub fn review_queue(ctx: &RepoContext, agent: &str) -> Queue {
    queue(ctx, format!("review:{agent}"))
}

/// The queue `agent`'s conflicts wait in.
pub fn resolve_queue(ctx: &RepoContext, agent: &str) -> Queue {
    queue(ctx, format!("resolve:{agent}"))
}

fn queue(ctx: &RepoContext, name: String) -> Queue {
    Queue::new(ctx.repo.clone(), name).with_policy(RetryPolicy {
        lease: LEASE,
        ..RetryPolicy::default()
    })
}

/// A queued job's payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetJob {
    pub change: ChangeId,
    pub version: ChangeVersionHash,
    pub reason: String,
}

impl FleetJob {
    pub fn decode(payload: &[u8]) -> Result<Self> {
        serde_json::from_slice(payload).map_err(|e| Error::internal(format!("fleet job: {e}")))
    }
}

/// Count one run against `agent`'s daily budget in this repository;
/// false when the budget is spent.
async fn take_budget(ctx: &RepoContext, agent: &str, per_day: Option<u64>) -> Result<bool> {
    let Some(limit) = per_day else {
        return Ok(true);
    };
    let key = format!("reviewer-day:{agent}:{}", day_of(Timestamp::now()));
    for _ in 0..rill_review::lifecycle::MAX_WRITE_ATTEMPTS {
        let current = ctx.repo.get_misc(&key).await?;
        let used = current
            .as_deref()
            .and_then(|b| std::str::from_utf8(b).ok()?.parse::<u64>().ok())
            .unwrap_or(0);
        if used >= limit {
            return Ok(false);
        }
        let next = Bytes::from((used + 1).to_string());
        match ctx.repo.set_misc_cas(&key, current, next).await {
            Ok(()) => return Ok(true),
            Err(e) if e.is_conflict() => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(rill_store::Error::Conflict(format!("{key} kept changing")).into())
}

async fn enqueue(
    ctx: &RepoContext,
    kind: RunKind,
    queue: Queue,
    agent: &str,
    job: FleetJob,
) -> Result<ReviewRun> {
    let review = ctx.review();
    let existing = review
        .review_runs(&job.change)
        .await?
        .into_iter()
        .rev()
        .find(|r| r.is(kind, agent, &job.version) && r.status != RunStatus::Failed);
    if let Some(run) = existing {
        return Ok(run);
    }
    let mut run = ReviewRun::queued(kind, agent, job.version, job.reason.clone());
    let person = review.person(&Principal::agent(agent)).await?;
    let demoted = review
        .reviewer_stats()
        .await?
        .get(agent)
        .is_some_and(|s| s.demoted);
    match (&person.reviewer, kind) {
        (None, RunKind::Review) => run.finish(RunStatus::Skipped, "not a registered reviewer"),
        _ if demoted => run.finish(RunStatus::Skipped, "demoted for its track record"),
        (profile, _) => {
            let per_day = profile.as_ref().and_then(|p| p.runs_per_day);
            if !take_budget(ctx, agent, per_day).await? {
                run.finish(RunStatus::Skipped, "the reviewer's daily budget is spent");
            }
        }
    }
    if run.status == RunStatus::Queued {
        let payload = serde_json::to_vec(&job).map_err(|e| Error::internal(e.to_string()))?;
        let key = format!("{}:{}", job.change, job.version);
        let kind_name = match kind {
            RunKind::Review => "review",
            RunKind::Resolve => "resolve",
        };
        queue.push(JobSpec::new(kind_name, key, payload)).await?;
    }
    let recorded = run.clone();
    review
        .update_review_runs(&job.change, |runs| runs.push(recorded.clone()))
        .await?;
    tracing::info!(change = %job.change, agent, status = ?run.status, "fleet job queued");
    Ok(run)
}

/// Queue `agent`'s review of `version`, unless one is queued or done.
pub async fn enqueue_review(
    ctx: &RepoContext,
    agent: &str,
    change: ChangeId,
    version: ChangeVersionHash,
    reason: &str,
) -> Result<ReviewRun> {
    let job = FleetJob {
        change,
        version,
        reason: reason.to_string(),
    };
    enqueue(ctx, RunKind::Review, review_queue(ctx, agent), agent, job).await
}

/// Queue `agent`'s resolution of `version`'s conflicts.
pub async fn enqueue_resolve(
    ctx: &RepoContext,
    agent: &str,
    change: ChangeId,
    version: ChangeVersionHash,
) -> Result<ReviewRun> {
    let job = FleetJob {
        change,
        version,
        reason: "conflicts".into(),
    };
    enqueue(ctx, RunKind::Resolve, resolve_queue(ctx, agent), agent, job).await
}
