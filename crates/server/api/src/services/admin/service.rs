//! The `AdminService` impl: dispatches each RPC to its handler.

use std::sync::Arc;

use connectrpc::{RequestContext, Response, ServiceRequest};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::AdminService;

use crate::services::common::GrpcResult;
use crate::services::Call;
use rill_runtime::state::AppState;

/// The `AdminService`.
#[derive(Clone)]
pub struct AdminSvc {
    pub(super) state: Arc<AppState>,
}

impl AdminSvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        AdminSvc { state }
    }
}

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl AdminService for AdminSvc {
    async fn get_health(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetHealthRequest>,
    ) -> GrpcResult<pb::GetHealthResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_health_impl(request).await?))
    }

    async fn import_trunk_position(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ImportTrunkPositionRequest>,
    ) -> GrpcResult<pb::ImportTrunkPositionResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.import_trunk_position_impl(r).await?))
            })
            .await?,
        )
    }
}
