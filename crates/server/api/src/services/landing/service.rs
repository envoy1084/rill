//! The `LandingService` impl: dispatches each RPC to its handler.

use std::sync::Arc;

use connectrpc::{RequestContext, Response, ServiceRequest};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::LandingService;

use crate::services::common::GrpcResult;
use crate::services::Call;
use rill_runtime::state::AppState;

/// The `LandingService`.
#[derive(Clone)]
pub struct LandingSvc {
    pub(super) state: Arc<AppState>,
}

impl LandingSvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        LandingSvc { state }
    }
}

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl LandingService for LandingSvc {
    async fn request_land(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::RequestLandRequest>,
    ) -> GrpcResult<pb::RequestLandResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.request_land_impl(r).await?))
            })
            .await?,
        )
    }

    async fn request_land_stack(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::RequestLandStackRequest>,
    ) -> GrpcResult<pb::RequestLandStackResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.request_land_stack_impl(r).await?))
            })
            .await?,
        )
    }

    async fn get_land_request(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetLandRequestRequest>,
    ) -> GrpcResult<pb::GetLandRequestResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_land_request_impl(request).await?))
    }

    async fn list_land_requests(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListLandRequestsRequest>,
    ) -> GrpcResult<pb::ListLandRequestsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_land_requests_impl(request).await?))
    }
}
