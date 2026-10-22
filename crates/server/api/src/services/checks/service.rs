//! The `ChecksService` impl: dispatches each RPC to its handler.

use std::sync::Arc;

use connectrpc::{RequestContext, Response, ServiceRequest};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::ChecksService;

use super::runs::RunStream;
use crate::services::common::GrpcResult;
use crate::services::Call;
use rill_runtime::state::AppState;

/// The `ChecksService`.
#[derive(Clone)]
pub struct ChecksSvc {
    pub(super) state: Arc<AppState>,
}

impl ChecksSvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        ChecksSvc { state }
    }
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
        Ok(Response::new(self.get_test_history_impl(request).await?))
    }

    async fn list_flaky_tests(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListFlakyTestsRequest>,
    ) -> GrpcResult<pb::ListFlakyTestsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_flaky_tests_impl(request).await?))
    }

    async fn list_runs(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListRunsRequest>,
    ) -> GrpcResult<pb::ListRunsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_runs_impl(request).await?))
    }

    async fn get_run(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetRunRequest>,
    ) -> GrpcResult<pb::GetRunResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_run_impl(request).await?))
    }

    async fn watch_run(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::WatchRunRequest>,
    ) -> GrpcResult<RunStream> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.watch_run_impl(request).await?))
    }

    async fn read_log(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ReadLogRequest>,
    ) -> GrpcResult<pb::ReadLogResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.read_log_impl(request).await?))
    }

    async fn read_artifact(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ReadArtifactRequest>,
    ) -> GrpcResult<pb::ReadArtifactResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.read_artifact_impl(request).await?))
    }

    async fn list_nonhermetic_steps(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListNonhermeticStepsRequest>,
    ) -> GrpcResult<pb::ListNonhermeticStepsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(
            self.list_nonhermetic_steps_impl(request).await?,
        ))
    }

    async fn list_quarantines(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListQuarantinesRequest>,
    ) -> GrpcResult<pb::ListQuarantinesResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_quarantines_impl(request).await?))
    }

    async fn create_quarantine(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::CreateQuarantineRequest>,
    ) -> GrpcResult<pb::CreateQuarantineResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.create_quarantine_impl(r).await?))
            })
            .await?,
        )
    }

    async fn delete_quarantine(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::DeleteQuarantineRequest>,
    ) -> GrpcResult<pb::DeleteQuarantineResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.delete_quarantine_impl(r).await?))
            })
            .await?,
        )
    }
}
