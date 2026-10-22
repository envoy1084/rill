//! The service and its RPC entry points.

use std::sync::Arc;

use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::checks_service_server::ChecksService;
use rill_runtime::state::AppState;
use tonic::{Request, Response, Status};

use super::runs::RunStream;

type GrpcResult<T> = std::result::Result<Response<T>, Status>;

/// The checks service.
#[derive(Clone)]
pub struct ChecksSvc {
    pub(super) state: Arc<AppState>,
}

impl ChecksSvc {
    pub fn new(state: Arc<AppState>) -> Self {
        ChecksSvc { state }
    }
}

fn reply<T>(result: rill_runtime::error::Result<T>) -> GrpcResult<T> {
    result.map(Response::new).map_err(Status::from)
}

#[tonic::async_trait]
impl ChecksService for ChecksSvc {
    type WatchRunStream = RunStream;

    async fn get_test_history(
        &self,
        request: Request<pb::GetTestHistoryRequest>,
    ) -> GrpcResult<pb::GetTestHistoryResponse> {
        reply(self.history(request).await)
    }

    async fn list_flaky_tests(
        &self,
        request: Request<pb::ListFlakyTestsRequest>,
    ) -> GrpcResult<pb::ListFlakyTestsResponse> {
        reply(self.flaky(request).await)
    }

    async fn list_runs(
        &self,
        request: Request<pb::ListRunsRequest>,
    ) -> GrpcResult<pb::ListRunsResponse> {
        reply(self.list_runs(request).await)
    }

    async fn get_run(&self, request: Request<pb::GetRunRequest>) -> GrpcResult<pb::VerifyRun> {
        reply(self.get_run(request).await)
    }

    async fn watch_run(&self, request: Request<pb::GetRunRequest>) -> GrpcResult<RunStream> {
        reply(self.watch_run(request).await)
    }

    async fn read_log(
        &self,
        request: Request<pb::ReadLogRequest>,
    ) -> GrpcResult<pb::ReadBytesResponse> {
        reply(self.read_log(request).await)
    }

    async fn read_artifact(
        &self,
        request: Request<pb::ReadArtifactRequest>,
    ) -> GrpcResult<pb::ReadBytesResponse> {
        reply(self.read_artifact(request).await)
    }

    async fn list_nonhermetic(
        &self,
        request: Request<pb::ListNonhermeticRequest>,
    ) -> GrpcResult<pb::ListNonhermeticResponse> {
        reply(self.nonhermetic(request).await)
    }

    async fn list_quarantined(
        &self,
        request: Request<pb::ListQuarantinedRequest>,
    ) -> GrpcResult<pb::ListQuarantinedResponse> {
        reply(self.quarantined(request).await)
    }

    async fn quarantine(
        &self,
        request: Request<pb::QuarantineRequest>,
    ) -> GrpcResult<pb::QuarantineResponse> {
        crate::services::idem::once(&self.state, request, |r| async move {
            self.quarantine_impl(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn release_quarantine(
        &self,
        request: Request<pb::ReleaseQuarantineRequest>,
    ) -> GrpcResult<pb::ReleaseQuarantineResponse> {
        crate::services::idem::once(&self.state, request, |r| async move {
            self.release_impl(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }
}
