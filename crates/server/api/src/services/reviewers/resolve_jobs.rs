//! Leasing and completing resolve jobs. The resolver stores its
//! resolution through `ChangeService.ResolveConflicts` like anyone else
//! (so its write rights apply); completing only records the outcome.

use rill_jobs::Claim;
use rill_proto::rill::v1 as pb;
use rill_review::agents::{RunKind, RunStatus};
use rill_runtime::error::{Error, Result};
use rill_runtime::fleet::{resolve_queue, FleetJob};
use rill_types::{RepoPathBuf, ReviewStatus};

use super::lease::{self, agent_name, finish_run, still_queued};
use super::runs::run_proto;
use super::ReviewerSvc;
use crate::services::common::{open_repo, Opened};
use crate::services::{names, Call};

async fn offer(
    opened: &Opened,
    agent: &str,
    claim: Claim,
    job: FleetJob,
) -> Result<Option<(Claim, pb::ResolveJob)>> {
    let queue = resolve_queue(&opened.repo, agent);
    if !still_queued(opened, RunKind::Resolve, agent, &job).await? {
        queue.complete(&claim).await?;
        return Ok(None);
    }
    let review = opened.repo.review();
    let state = review.get(&job.change).await?;
    let cv = opened.repo.derive.change(&job.version).await?;
    let open = matches!(state.status, ReviewStatus::Open | ReviewStatus::Landable);
    if !open || state.current_version() != Some(job.version) || !cv.has_conflicts() {
        finish_run(opened, RunKind::Resolve, agent, &job, |r| {
            r.finish(RunStatus::Stale, "the conflict is gone")
        })
        .await?;
        queue.complete(&claim).await?;
        return Ok(None);
    }
    let prompt = RepoPathBuf::new(format!(".rill/reviewers/{agent}/prompt.md"))?;
    let prompt = review.trunk_file(&prompt).await?;
    let job = pb::ResolveJob {
        change: names::change(&opened.repo, &job.change),
        version: names::version(&opened.repo, &job.version),
        paths: cv.conflicted_paths().map(|p| p.to_string()).collect(),
        prompt: prompt
            .map(|p| String::from_utf8_lossy(&p).into_owned())
            .unwrap_or_default(),
    };
    Ok(Some((claim, job)))
}

impl ReviewerSvc {
    pub(super) async fn lease_resolve_job_impl(
        &self,
        request: Call<pb::LeaseResolveJobRequest>,
    ) -> Result<pb::LeaseResolveJobResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().write_any().await?;
        let agent = agent_name(&opened)?;
        let queue = resolve_queue(&opened.repo, &agent);
        let found = lease::next(
            &queue,
            opened.principal(),
            lease::wait_for(req.wait_ms),
            |c, j| {
                let (opened, agent) = (&opened, agent.as_str());
                async move { offer(opened, agent, c, j).await }
            },
        )
        .await?;
        let Some((claim, job)) = found else {
            return Ok(pb::LeaseResolveJobResponse::default());
        };
        let (lease, lease_until) = lease::encode(&queue, &claim)?;
        Ok(pb::LeaseResolveJobResponse {
            lease: lease.into(),
            lease_until,
            job: job.into(),
        })
    }

    pub(super) async fn complete_resolve_job_impl(
        &self,
        request: Call<pb::CompleteResolveJobRequest>,
    ) -> Result<pb::CompleteResolveJobResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().write_any().await?;
        let agent = agent_name(&opened)?;
        let queue = resolve_queue(&opened.repo, &agent);
        let claim = lease::decode(&queue, opened.principal(), &req.lease)?;
        let claim = queue.renew(&claim).await?;
        let job = FleetJob::decode(&claim.job.payload)?;
        let detail = if req.resolved_version.is_empty() {
            format!("declined: {}", req.rationale)
        } else {
            let resolved = names::version_in(&opened.repo, &req.resolved_version)?;
            let cv = opened.repo.derive.change(&resolved).await?;
            if cv.change_id != job.change || cv.has_conflicts() {
                return Err(Error::invalid(format!(
                    "{resolved} is not a resolved version of {}",
                    job.change
                )));
            }
            format!("resolved as {resolved}: {}", req.rationale)
        };
        let run = finish_run(&opened, RunKind::Resolve, &agent, &job, |r| {
            r.finish(RunStatus::Done, detail.clone())
        })
        .await?;
        queue.complete(&claim).await?;
        Ok(pb::CompleteResolveJobResponse {
            run: run_proto(&opened.repo, &run).into(),
        })
    }
}
