//! Reviewers' track records: listing them, and reinstating a demoted
//! reviewer.

use rill_policy::reviewers::ReviewerStats;
use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_runtime::error::Result;
use rill_types::{RepoPathBuf, Timestamp};

use super::ReviewerSvc;
use crate::services::common::open_repo;
use crate::services::Call;

fn stats_proto(agent: &str, s: &ReviewerStats) -> pb::ReviewerStats {
    pb::ReviewerStats {
        reviewer: format!("agent:{agent}"),
        trust: s.trust().into(),
        outcomes: s.outcomes() as u32,
        approvals: s.approvals,
        escapes: s.escapes,
        blocks: s.blocks,
        dismissed: s.dismissed,
        demoted: s.demoted,
        note: s.note.clone(),
        since: s.since.map(timestamp_to_proto).unwrap_or_default(),
    }
}

impl ReviewerSvc {
    pub(super) async fn list_reviewer_stats_impl(
        &self,
        request: Call<pb::ListReviewerStatsRequest>,
    ) -> Result<pb::ListReviewerStatsResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let stats = opened.repo.review().reviewer_stats().await?;
        Ok(pb::ListReviewerStatsResponse {
            reviewers: stats.iter().map(|(a, s)| stats_proto(a, s)).collect(),
        })
    }

    pub(super) async fn reinstate_reviewer_impl(
        &self,
        request: Call<pb::ReinstateReviewerRequest>,
    ) -> Result<pb::ReinstateReviewerResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().admin().await?;
        let agent = req
            .reviewer
            .strip_prefix("agent:")
            .unwrap_or(&req.reviewer)
            .to_string();
        let by = opened.principal().to_string();
        let now = Timestamp::now();
        let stats = opened
            .repo
            .review()
            .update_reviewer_stats(|map| {
                let s = map.entry(agent.clone()).or_default();
                s.reinstate(&by, now);
                s.clone()
            })
            .await?;
        tracing::info!(reviewer = %agent, by = %by, "reviewer reinstated");
        Ok(pb::ReinstateReviewerResponse {
            stats: stats_proto(&agent, &stats).into(),
        })
    }
}
