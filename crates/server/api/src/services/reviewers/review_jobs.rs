//! Leasing, renewing, completing and failing review jobs.

use std::collections::BTreeSet;

use rill_jobs::Claim;
use rill_policy::reviewers::REVIEW_CONFIG;
use rill_proto::convert::{comment_category_from_proto, confidence_from_proto};
use rill_proto::rill::v1 as pb;
use rill_review::agents::{check_result, RunKind, RunStatus};
use rill_review::{Budget, Finding, ReviewResult};
use rill_runtime::error::{Error, Result};
use rill_runtime::fleet::{review_queue, FleetJob};
use rill_types::identity::ReviewerProfile;
use rill_types::{RepoPathBuf, ReviewStatus};

use super::lease::{self, agent_name, finish_run, still_queued};
use super::packet::packet;
use super::runs::run_proto;
use super::ReviewerSvc;
use crate::services::common::{open_repo, Opened};
use crate::services::identity::agents::profile_proto;
use crate::services::{names, Call};

/// The caller's reviewer profile.
async fn profile(opened: &Opened) -> Result<ReviewerProfile> {
    opened
        .repo
        .review()
        .person(opened.principal())
        .await?
        .reviewer
        .ok_or_else(|| Error::precondition(format!("{} is not a reviewer", opened.principal())))
}

/// Offer a claimed job: end it when it needs no work (a duplicate, a
/// change that moved on, paths the reviewer cannot read), else build what
/// the reviewer is sent.
async fn offer(
    opened: &Opened,
    agent: &str,
    claim: Claim,
    job: FleetJob,
) -> Result<Option<(Claim, pb::ReviewJob)>> {
    let queue = review_queue(&opened.repo, agent);
    if !still_queued(opened, RunKind::Review, agent, &job).await? {
        queue.complete(&claim).await?;
        return Ok(None);
    }
    let review = opened.repo.review();
    let state = review.get(&job.change).await?;
    let open = matches!(state.status, ReviewStatus::Open | ReviewStatus::Landable);
    if !open || state.current_version() != Some(job.version) {
        finish_run(opened, RunKind::Review, agent, &job, |r| {
            r.finish(
                RunStatus::Stale,
                "the change moved on before the review ran",
            )
        })
        .await?;
        queue.complete(&claim).await?;
        return Ok(None);
    }
    for path in review.touched_paths(&job.version).await? {
        if opened.permit().read(&path).await.is_err() {
            finish_run(opened, RunKind::Review, agent, &job, |r| {
                r.finish(
                    RunStatus::Failed,
                    format!("the reviewer cannot read {path}"),
                )
            })
            .await?;
            queue.fail(&claim, "unreadable path", true).await?;
            return Ok(None);
        }
    }
    let profile = profile(opened).await?;
    let ctx = review
        .review_context(&job.change, job.version, Budget::default())
        .await?;
    let prompt = match RepoPathBuf::new(&profile.prompt) {
        Ok(path) => review.trunk_file(&path).await?,
        Err(_) => None,
    };
    let job = pb::ReviewJob {
        change: names::change(&opened.repo, &job.change),
        version: names::version(&opened.repo, &job.version),
        reason: job.reason,
        profile: profile_proto(&profile).into(),
        prompt: prompt
            .map(|p| String::from_utf8_lossy(&p).into_owned())
            .unwrap_or_default(),
        packet: packet(&ctx, state.risk).into(),
    };
    Ok(Some((claim, job)))
}

fn finding(f: &pb::ReviewFinding) -> Result<Finding> {
    Ok(Finding {
        path: match f.path.as_str() {
            "" => None,
            p => Some(RepoPathBuf::new(p).map_err(|e| Error::invalid(e.to_string()))?),
        },
        lines: match (f.start_line, f.end_line) {
            (0, 0) => None,
            (s, e) => Some((s, e)),
        },
        category: comment_category_from_proto("findings.category", f.category)?,
        confidence: confidence_from_proto("findings.confidence", f.confidence)?,
        body: f.body.clone(),
    })
}

impl ReviewerSvc {
    pub(super) async fn lease_review_job_impl(
        &self,
        request: Call<pb::LeaseReviewJobRequest>,
    ) -> Result<pb::LeaseReviewJobResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().review().await?;
        let agent = agent_name(&opened)?;
        profile(&opened).await?;
        let queue = review_queue(&opened.repo, &agent);
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
            return Ok(pb::LeaseReviewJobResponse::default());
        };
        let (lease, lease_until) = lease::encode(&queue, &claim)?;
        Ok(pb::LeaseReviewJobResponse {
            lease: lease.into(),
            lease_until,
            job: job.into(),
        })
    }

    pub(super) async fn renew_review_lease_impl(
        &self,
        request: Call<pb::RenewReviewLeaseRequest>,
    ) -> Result<pb::RenewReviewLeaseResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().review().await?;
        let agent = agent_name(&opened)?;
        let queue = self.queue_of(&opened, &agent, &req.lease)?;
        let claim = lease::decode(&queue, opened.principal(), &req.lease)?;
        let claim = queue.renew(&claim).await?;
        let (lease, lease_until) = lease::encode(&queue, &claim)?;
        Ok(pb::RenewReviewLeaseResponse {
            lease: lease.into(),
            lease_until,
        })
    }

    pub(super) async fn complete_review_job_impl(
        &self,
        request: Call<pb::CompleteReviewJobRequest>,
    ) -> Result<pb::CompleteReviewJobResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().review().await?;
        let agent = agent_name(&opened)?;
        let queue = review_queue(&opened.repo, &agent);
        let claim = lease::decode(&queue, opened.principal(), &req.lease)?;
        let claim = queue.renew(&claim).await?;
        let job = FleetJob::decode(&claim.job.payload)?;
        let profile = profile(&opened).await?;
        let review = opened.repo.review();
        let result = ReviewResult {
            version: job.version,
            findings: req.findings.iter().map(finding).collect::<Result<_>>()?,
            vote: i8::try_from(req.vote).map_err(|_| Error::invalid("vote out of range"))?,
            summary: req.summary.clone(),
        };
        let touched: BTreeSet<RepoPathBuf> = review.touched_paths(&job.version).await?;
        if let Err(e) = check_result(&result, profile.max_vote, &touched) {
            finish_run(&opened, RunKind::Review, &agent, &job, |r| {
                r.model = req.model.clone();
                r.finish(RunStatus::Failed, e.to_string())
            })
            .await?;
            queue.fail(&claim, &e.to_string(), true).await?;
            return Err(e.into());
        }
        let posted = review
            .post_review_result(&job.change, opened.principal(), &result)
            .await;
        let run = finish_run(&opened, RunKind::Review, &agent, &job, |r| {
            r.model = req.model.clone();
            match &posted {
                Ok(p) => {
                    r.findings = p.threads.len() as u32;
                    r.vote = p.vote;
                    r.finish(RunStatus::Done, req.summary.clone());
                }
                Err(rill_review::Error::StaleVersion { .. }) => {
                    r.finish(RunStatus::Stale, "the change moved on during the review")
                }
                Err(e) => r.finish(RunStatus::Failed, e.to_string()),
            }
        })
        .await?;
        queue.complete(&claim).await?;
        if let Err(e) = posted {
            if !matches!(e, rill_review::Error::StaleVersion { .. }) {
                return Err(e.into());
            }
        }
        tracing::info!(change = %job.change, reviewer = %agent, vote = run.vote, findings = run.findings, "review posted");
        Ok(pb::CompleteReviewJobResponse {
            run: run_proto(&opened.repo, &run).into(),
        })
    }

    pub(super) async fn fail_review_job_impl(
        &self,
        request: Call<pb::FailReviewJobRequest>,
    ) -> Result<pb::FailReviewJobResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().review().await?;
        let agent = agent_name(&opened)?;
        let queue = self.queue_of(&opened, &agent, &req.lease)?;
        let claim = lease::decode(&queue, opened.principal(), &req.lease)?;
        let job = FleetJob::decode(&claim.job.payload)?;
        let kind = if queue.name().starts_with("resolve:") {
            RunKind::Resolve
        } else {
            RunKind::Review
        };
        queue.fail(&claim, &req.error, !req.retry).await?;
        if !req.retry {
            finish_run(&opened, kind, &agent, &job, |r| {
                r.finish(RunStatus::Failed, req.error.clone())
            })
            .await?;
        }
        Ok(pb::FailReviewJobResponse::default())
    }

    /// The caller's queue a lease belongs to: its review or resolve queue.
    fn queue_of(&self, opened: &Opened, agent: &str, lease: &[u8]) -> Result<rill_jobs::Queue> {
        let review = review_queue(&opened.repo, agent);
        if lease::decode(&review, opened.principal(), lease).is_ok() {
            return Ok(review);
        }
        Ok(rill_runtime::fleet::resolve_queue(&opened.repo, agent))
    }
}

/// Why a lease's change cannot be routed, for `RequestReview`.
pub(super) fn no_routing() -> Error {
    Error::precondition(format!(
        "{REVIEW_CONFIG} on trunk routes this change to no reviewer; name reviewers"
    ))
}
