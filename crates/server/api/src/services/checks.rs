//! `rill.v1.ChecksService`: the test history verification records and
//! flake scores, for anyone who can read the repository.

use std::sync::Arc;

use rill_exec::history::{FlakeStats, FlakyTarget, TestHistory, TestOutcome, TestRun, RUNS_KEPT};
use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::checks_service_server::ChecksService;
use rill_types::Timestamp;
use tonic::{Request, Response, Status};

use super::common::open;
use rill_runtime::error::Result;
use rill_runtime::state::AppState;

type GrpcResult<T> = std::result::Result<Response<T>, Status>;

/// Default and largest page of runs.
const DEFAULT_RUNS: usize = 50;
/// Default flake score floor and page of flaky targets.
const DEFAULT_MIN_SCORE: f64 = 0.1;
const DEFAULT_FLAKY: usize = 50;

/// The checks service.
#[derive(Clone)]
pub struct ChecksSvc {
    state: Arc<AppState>,
}

impl ChecksSvc {
    pub fn new(state: Arc<AppState>) -> Self {
        ChecksSvc { state }
    }
}

fn outcome_proto(outcome: TestOutcome) -> i32 {
    (match outcome {
        TestOutcome::Passed => pb::TestOutcome::Passed,
        TestOutcome::Failed => pb::TestOutcome::Failed,
        TestOutcome::Skipped => pb::TestOutcome::Skipped,
        TestOutcome::Error => pb::TestOutcome::Error,
    }) as i32
}

fn run_proto(run: TestRun) -> pb::TestRun {
    pb::TestRun {
        target: run.target,
        outcome: outcome_proto(run.outcome),
        retried: run.retried,
        duration_ms: run.duration_ms,
        action: run
            .action
            .map(|a| bytes::Bytes::copy_from_slice(a.as_bytes()))
            .unwrap_or_default(),
        worker: run.worker.unwrap_or_default(),
        version: bytes::Bytes::copy_from_slice(run.version.as_bytes()),
        trunk: run.trunk,
        at: timestamp_to_proto(run.at),
    }
}

fn flaky_proto(target: String, score: f64, stats: FlakeStats) -> pb::FlakyTest {
    pb::FlakyTest {
        target,
        score,
        runs: stats.runs,
        failures: stats.failures,
        flips: stats.flips,
        last: stats.last.map_or(0, outcome_proto),
    }
}

impl ChecksSvc {
    async fn history(
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
            flaky_proto(
                req.target.clone(),
                stats.score(Timestamp::now().millis()),
                stats,
            )
        });
        Ok(pb::GetTestHistoryResponse {
            runs: runs.into_iter().map(run_proto).collect(),
            stats,
        })
    }

    async fn flaky(
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
                     }| flaky_proto(target, score, stats),
                )
                .collect(),
        })
    }
}

#[tonic::async_trait]
impl ChecksService for ChecksSvc {
    async fn get_test_history(
        &self,
        request: Request<pb::GetTestHistoryRequest>,
    ) -> GrpcResult<pb::GetTestHistoryResponse> {
        self.history(request)
            .await
            .map(Response::new)
            .map_err(Status::from)
    }

    async fn list_flaky_tests(
        &self,
        request: Request<pb::ListFlakyTestsRequest>,
    ) -> GrpcResult<pb::ListFlakyTestsResponse> {
        self.flaky(request)
            .await
            .map(Response::new)
            .map_err(Status::from)
    }
}
