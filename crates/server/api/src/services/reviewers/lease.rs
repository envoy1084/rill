//! Leases as runners hold them: an opaque, serialized claim on a job of
//! the caller's own queue, checked whenever it comes back.

use std::time::{Duration, Instant};

use rill_jobs::{Claim, Queue};
use rill_proto::convert::timestamp_to_proto;
use rill_review::agents::{ReviewRun, RunKind, RunStatus};
use rill_runtime::error::{Error, Result};
use rill_runtime::fleet::{FleetJob, LEASE_WAIT_MAX};
use rill_types::{Principal, PrincipalKind};
use serde::{Deserialize, Serialize};

use crate::services::common::Opened;

#[derive(Serialize, Deserialize)]
struct Wire {
    queue: String,
    claim: Claim,
}

/// The calling agent's name; only agents hold fleet leases.
pub(super) fn agent_name(opened: &Opened) -> Result<String> {
    let me: &Principal = opened.principal();
    if me.kind != PrincipalKind::Agent {
        return Err(Error::precondition(
            "only reviewer and resolver agents lease fleet jobs",
        ));
    }
    Ok(me.name.clone())
}

/// A lease's wire form and its expiry.
pub(super) fn encode(queue: &Queue, claim: &Claim) -> Result<(Vec<u8>, i64)> {
    let wire = Wire {
        queue: queue.name().to_string(),
        claim: claim.clone(),
    };
    let bytes = serde_json::to_vec(&wire).map_err(|e| Error::internal(e.to_string()))?;
    Ok((bytes, timestamp_to_proto(claim.lease_until)))
}

/// The claim in `bytes`, when it is one `queue` issued to `holder`.
pub(super) fn decode(queue: &Queue, holder: &Principal, bytes: &[u8]) -> Result<Claim> {
    let wire: Wire = serde_json::from_slice(bytes).map_err(|_| Error::invalid("bad lease"))?;
    if wire.queue != queue.name() || wire.claim.holder != holder.to_string() {
        return Err(Error::denied("the lease is not the caller's"));
    }
    queue.verify(&wire.claim)?;
    Ok(wire.claim)
}

/// How long a lease request may wait.
pub(super) fn wait_for(wait_ms: u32) -> Duration {
    Duration::from_millis(wait_ms.into()).min(LEASE_WAIT_MAX)
}

/// Claim the next job of `queue` for `holder`, waiting until `wait` for
/// one to be ready. Each claim is offered to `usable`, which ends jobs
/// that need no work and returns `None` for them.
pub(super) async fn next<T, F, Fut>(
    queue: &Queue,
    holder: &Principal,
    wait: Duration,
    mut usable: F,
) -> Result<Option<(Claim, T)>>
where
    F: FnMut(Claim, FleetJob) -> Fut,
    Fut: std::future::Future<Output = Result<Option<(Claim, T)>>>,
{
    let deadline = Instant::now() + wait;
    loop {
        queue.reap().await?;
        let seen = queue.wake_count().await?;
        while let Some(claim) = queue.claim(&holder.to_string()).await? {
            let job = FleetJob::decode(&claim.job.payload)?;
            if let Some(found) = usable(claim, job).await? {
                return Ok(Some(found));
            }
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ok(None);
        }
        queue.wait(seen, left).await?;
    }
}

/// Finish `agent`'s `kind` run on the job's version with `f`.
pub(super) async fn finish_run(
    opened: &Opened,
    kind: RunKind,
    agent: &str,
    job: &FleetJob,
    f: impl Fn(&mut ReviewRun),
) -> Result<ReviewRun> {
    let review = opened.repo.review();
    let run = review
        .update_review_runs(&job.change, |runs| {
            let found = runs
                .iter_mut()
                .rev()
                .find(|r| r.is(kind, agent, &job.version) && r.status == RunStatus::Queued);
            match found {
                Some(run) => {
                    f(run);
                    run.clone()
                }
                None => {
                    let mut run = ReviewRun::queued(kind, agent, job.version, job.reason.clone());
                    f(&mut run);
                    runs.push(run.clone());
                    run
                }
            }
        })
        .await?;
    Ok(run)
}

/// Whether `agent` still has a queued `kind` run for the job: one already
/// finished (a duplicate job) needs no work.
pub(super) async fn still_queued(
    opened: &Opened,
    kind: RunKind,
    agent: &str,
    job: &FleetJob,
) -> Result<bool> {
    Ok(opened
        .repo
        .review()
        .review_runs(&job.change)
        .await?
        .iter()
        .any(|r| r.is(kind, agent, &job.version) && r.status == RunStatus::Queued))
}
