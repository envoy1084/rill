//! The `OperationService` impl: dispatches each RPC to its handler.

use std::sync::Arc;

use connectrpc::{RequestContext, Response, ServiceRequest};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::OperationService;

use crate::services::common::GrpcResult;
use crate::services::Call;
use rill_runtime::state::AppState;

/// The `OperationService`.
#[derive(Clone)]
pub struct OperationSvc {
    pub(super) state: Arc<AppState>,
}

impl OperationSvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        OperationSvc { state }
    }
}

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl OperationService for OperationSvc {
    async fn get_operation(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetOperationRequest>,
    ) -> GrpcResult<pb::GetOperationResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_operation_impl(request).await?))
    }

    async fn list_operations(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListOperationsRequest>,
    ) -> GrpcResult<pb::ListOperationsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_operations_impl(request).await?))
    }

    async fn wait_operation(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::WaitOperationRequest>,
    ) -> GrpcResult<pb::WaitOperationResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.wait_operation_impl(request).await?))
    }
}
