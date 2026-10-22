//! Test history and flake scores.

use rill_exec::history::{FlakyTarget, TestHistory, RUNS_KEPT};
use rill_proto::rill::v1 as pb;
use rill_types::Timestamp;

use super::convert;
use super::service::ChecksSvc;
use crate::listing::Page;
use crate::services::common::open_repo;
use crate::services::Call;
use rill_runtime::error::Result;

/// Default page of runs.
const DEFAULT_RUNS: usize = 50;
/// Default flake score floor.
const DEFAULT_MIN_SCORE: f64 = 0.1;

impl ChecksSvc {
    pub(super) async fn get_test_history_impl(
        &self,
        request: Call<pb::GetTestHistoryRequest>,
    ) -> Result<pb::GetTestHistoryResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().parent).await?;
        opened.permit().read_any().await?;
        let req = request.into_inner();
        if req.target.is_empty() {
            return Err(rill_proto::ConvertError::missing("target").into());
        }
        let limit = match req.max_results {
            n if n <= 0 => DEFAULT_RUNS,
            n => (n as usize).min(RUNS_KEPT),
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
            stats: stats.into(),
        })
    }

    pub(super) async fn list_flaky_tests_impl(
        &self,
        request: Call<pb::ListFlakyTestsRequest>,
    ) -> Result<pb::ListFlakyTestsResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().read_any().await?;
        let min_score = req.min_score.unwrap_or(DEFAULT_MIN_SCORE);
        let page = Page::new(
            req.page_size,
            &req.page_token,
            &[&req.parent, &min_score.to_string()],
        )?;
        let flaky = TestHistory::new(opened.repo.repo.clone())
            .flaky(min_score, usize::MAX)
            .await?;
        let (flaky, next_page_token) = page.of(flaky)?;
        Ok(pb::ListFlakyTestsResponse {
            next_page_token,
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
    pub(super) async fn list_nonhermetic_steps_impl(
        &self,
        request: Call<pb::ListNonhermeticStepsRequest>,
    ) -> Result<pb::ListNonhermeticStepsResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().read_any().await?;
        let page = Page::new(req.page_size, &req.page_token, &[&req.parent])?;
        let config = &opened.repo.config;
        let space = self.state.exec.space(&config.org);
        let prefix = format!("verify:{}/", config.name);
        let flags = self.state.exec.nonhermetics(&space, &prefix).await?;
        let (flags, next_page_token) = page.of(flags)?;
        Ok(pb::ListNonhermeticStepsResponse {
            next_page_token,
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
