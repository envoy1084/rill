//! Test history and flake scores.

use rill_exec::history::{FlakyTarget, TestHistory, RUNS_KEPT};
use rill_proto::rill::v1 as pb;
use rill_types::Timestamp;
use tonic::Request;

use super::convert;
use super::service::ChecksSvc;
use crate::services::common::open;
use rill_runtime::error::Result;

/// Default page of runs.
const DEFAULT_RUNS: usize = 50;
/// Default flake score floor and page of flaky targets.
const DEFAULT_MIN_SCORE: f64 = 0.1;
const DEFAULT_FLAKY: usize = 50;

impl ChecksSvc {
    pub(super) async fn history(
        &self,
        request: Request<pb::GetTestHistoryRequest>,
    ) -> Result<pb::GetTestHistoryResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read_any().await?;
        let req = request.into_inner();
        let limit = match req.limit as usize {
            0 => DEFAULT_RUNS,
            n => n.min(RUNS_KEPT),
        };
        let history = TestHistory::new(opened.repo.repo.clone());
        let runs = history.runs(&req.target, limit).await?;
        let stats = history.stats(&req.target).await?.map(|stats| {
            convert::flaky(
                req.target.clone(),
                stats.score(Timestamp::now().millis()),
                stats,
            )
        });
        Ok(pb::GetTestHistoryResponse {
            runs: runs.into_iter().map(convert::test_run).collect(),
            stats,
        })
    }

    pub(super) async fn flaky(
        &self,
        request: Request<pb::ListFlakyTestsRequest>,
    ) -> Result<pb::ListFlakyTestsResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read_any().await?;
        let req = request.into_inner();
        let limit = match req.limit as usize {
            0 => DEFAULT_FLAKY,
            n => n,
        };
        let flaky = TestHistory::new(opened.repo.repo.clone())
            .flaky(req.min_score.unwrap_or(DEFAULT_MIN_SCORE), limit)
            .await?;
        Ok(pb::ListFlakyTestsResponse {
            tests: flaky
                .into_iter()
                .map(
                    |FlakyTarget {
                         target,
                         score,
                         stats,
                     }| convert::flaky(target, score, stats),
                )
                .collect(),
        })
    }
}

impl ChecksSvc {
    pub(super) async fn nonhermetic(
        &self,
        request: Request<pb::ListNonhermeticRequest>,
    ) -> Result<pb::ListNonhermeticResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read_any().await?;
        let config = &opened.repo.config;
        let space = self.state.exec.space(&config.org);
        let prefix = format!("verify:{}/", config.name);
        let flags = self.state.exec.nonhermetics(&space, &prefix).await?;
        Ok(pb::ListNonhermeticResponse {
            steps: flags
                .into_iter()
                .map(|f| pb::NonhermeticStep {
                    step: f.label[prefix.len()..].to_string(),
                    action: bytes::Bytes::copy_from_slice(f.action.as_bytes()),
                    differences: f.differences,
                    found_at: rill_proto::convert::timestamp_to_proto(f.found_at),
                    until: rill_proto::convert::timestamp_to_proto(f.until),
                })
                .collect(),
        })
    }
}
