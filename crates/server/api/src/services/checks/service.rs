//! The service and its RPC entry points.

use std::sync::Arc;

use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::ChecksService;
use rill_runtime::state::AppState;

use super::runs::RunStream;
use crate::services::Call;
use connectrpc::{ConnectError, RequestContext, Response, ServiceRequest};

type GrpcResult<T> = std::result::Result<Response<T>, ConnectError>;

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
    result.map(Response::new).map_err(ConnectError::from)
}

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl ChecksService for ChecksSvc {
    async fn get_test_history(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetTestHistoryRequest>,
    ) -> GrpcResult<pb::GetTestHistoryResponse> {
        let request = Call::new(ctx, request);
        reply(self.history(request).await)
    }

    async fn list_flaky_tests(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListFlakyTestsRequest>,
    ) -> GrpcResult<pb::ListFlakyTestsResponse> {
        let request = Call::new(ctx, request);
        reply(self.flaky(request).await)
    }

    async fn list_runs(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListRunsRequest>,
    ) -> GrpcResult<pb::ListRunsResponse> {
        let request = Call::new(ctx, request);
        reply(self.list_runs(request).await)
    }

    async fn get_run(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetRunRequest>,
    ) -> GrpcResult<pb::VerifyRun> {
        let request = Call::new(ctx, request);
        reply(self.get_run(request).await)
    }

    async fn watch_run(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetRunRequest>,
    ) -> GrpcResult<RunStream> {
        let request = Call::new(ctx, request);
        reply(self.watch_run(request).await)
    }

    async fn read_log(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ReadLogRequest>,
    ) -> GrpcResult<pb::ReadBytesResponse> {
        let request = Call::new(ctx, request);
        reply(self.read_log(request).await)
    }

    async fn read_artifact(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ReadArtifactRequest>,
    ) -> GrpcResult<pb::ReadBytesResponse> {
        let request = Call::new(ctx, request);
        reply(self.read_artifact(request).await)
    }

    async fn list_nonhermetic(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListNonhermeticRequest>,
    ) -> GrpcResult<pb::ListNonhermeticResponse> {
        let request = Call::new(ctx, request);
        reply(self.nonhermetic(request).await)
    }

    async fn list_quarantined(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListQuarantinedRequest>,
    ) -> GrpcResult<pb::ListQuarantinedResponse> {
        let request = Call::new(ctx, request);
        reply(self.quarantined(request).await)
    }

    async fn quarantine(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::QuarantineRequest>,
    ) -> GrpcResult<pb::QuarantineResponse> {
        let request = Call::new(ctx, request);
        crate::services::idem::once(&self.state, request, |r| async move {
            self.quarantine_impl(r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn release_quarantine(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ReleaseQuarantineRequest>,
    ) -> GrpcResult<pb::ReleaseQuarantineResponse> {
        let request = Call::new(ctx, request);
        crate::services::idem::once(&self.state, request, |r| async move {
            self.release_impl(r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }
}
