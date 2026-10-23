//! Asking for reviews and listing a change's runs.

use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_review::agents::{ReviewRun, RunKind, RunStatus};
use rill_runtime::error::{Error, Result};
use rill_runtime::fleet::enqueue_review;
use rill_runtime::state::RepoContext;
use rill_types::{RepoPathBuf, ReviewStatus};

use super::review_jobs::no_routing;
use super::ReviewerSvc;
use crate::services::common::open_repo;
use crate::services::{names, Call};

/// A run on the wire.
pub(super) fn run_proto(ctx: &RepoContext, run: &ReviewRun) -> pb::ReviewRun {
    pb::ReviewRun {
        reviewer: format!("agent:{}", run.agent),
        version: names::version(ctx, &run.version),
        reason: run.reason.clone(),
        status: match run.status {
            RunStatus::Queued => pb::ReviewRunStatus::Queued,
            RunStatus::Done => pb::ReviewRunStatus::Done,
            RunStatus::Failed => pb::ReviewRunStatus::Failed,
            RunStatus::Stale => pb::ReviewRunStatus::Stale,
            RunStatus::Skipped => pb::ReviewRunStatus::Skipped,
        }
        .into(),
        queued_at: timestamp_to_proto(run.queued_at),
        finished_at: run.finished_at.map(timestamp_to_proto).unwrap_or_default(),
        findings: run.findings,
        vote: run.vote.into(),
        model: run.model.clone(),
        detail: run.detail.clone(),
        kind: match run.kind {
            RunKind::Review => "review",
            RunKind::Resolve => "resolve",
        }
        .into(),
    }
}

impl ReviewerSvc {
    pub(super) async fn request_review_impl(
        &self,
        request: Call<pb::RequestReviewRequest>,
    ) -> Result<pb::RequestReviewResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.change).await?;
        opened.permit().review().await?;
        let change = names::change_in(&opened.repo, &req.change)?;
        let review = opened.repo.review();
        let state = review.get(&change).await?;
        if !matches!(state.status, ReviewStatus::Open | ReviewStatus::Landable) {
            return Err(Error::precondition(format!("{change} is not in review")));
        }
        let version = state
            .current_version()
            .ok_or_else(|| Error::precondition(format!("{change} has no version")))?;
        let agents: Vec<String> = if req.reviewers.is_empty() {
            let config = review.trunk_review_config().await?;
            let paths: Vec<RepoPathBuf> =
                review.touched_paths(&version).await?.into_iter().collect();
            let refs: Vec<_> = paths.iter().map(|p| p.as_path()).collect();
            config
                .route(&refs, state.risk)
                .map(|r| r.agent.clone())
                .collect()
        } else {
            req.reviewers
                .iter()
                .map(|r| r.strip_prefix("agent:").unwrap_or(r).to_string())
                .collect()
        };
        if agents.is_empty() {
            return Err(no_routing());
        }
        let reason = format!("requested by {}", opened.principal());
        let mut runs = Vec::with_capacity(agents.len());
        for agent in agents {
            let run = enqueue_review(&opened.repo, &agent, change, version, &reason).await?;
            runs.push(run_proto(&opened.repo, &run));
        }
        Ok(pb::RequestReviewResponse { runs })
    }

    pub(super) async fn list_review_runs_impl(
        &self,
        request: Call<pb::ListReviewRunsRequest>,
    ) -> Result<pb::ListReviewRunsResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.change).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let change = names::change_in(&opened.repo, &req.change)?;
        let runs = opened.repo.review().review_runs(&change).await?;
        Ok(pb::ListReviewRunsResponse {
            runs: runs
                .iter()
                .rev()
                .map(|r| run_proto(&opened.repo, r))
                .collect(),
        })
    }
}
