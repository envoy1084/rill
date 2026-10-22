//! The `TrunkService` impl: dispatches each RPC to its handler.

use std::sync::Arc;

use connectrpc::{RequestContext, Response, ServiceRequest};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::TrunkService;

use crate::services::common::GrpcResult;
use crate::services::Call;
use rill_runtime::state::AppState;

/// The `TrunkService`.
#[derive(Clone)]
pub struct TrunkSvc {
    pub(super) state: Arc<AppState>,
}

impl TrunkSvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        TrunkSvc { state }
    }
}

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl TrunkService for TrunkSvc {
    async fn get_trunk_status(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetTrunkStatusRequest>,
    ) -> GrpcResult<pb::GetTrunkStatusResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_trunk_status_impl(request).await?))
    }

    async fn list_landings(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListLandingsRequest>,
    ) -> GrpcResult<pb::ListLandingsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_landings_impl(request).await?))
    }

    async fn get_verification(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetVerificationRequest>,
    ) -> GrpcResult<pb::GetVerificationResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_verification_impl(request).await?))
    }

    async fn report_verification(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ReportVerificationRequest>,
    ) -> GrpcResult<pb::ReportVerificationResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.report_verification_impl(r).await?))
            })
            .await?,
        )
    }

    async fn revert_landing(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::RevertLandingRequest>,
    ) -> GrpcResult<pb::RevertLandingResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.revert_landing_impl(r).await?))
            })
            .await?,
        )
    }

    async fn preview_bulk_revert(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::PreviewBulkRevertRequest>,
    ) -> GrpcResult<pb::PreviewBulkRevertResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.preview_bulk_revert_impl(request).await?))
    }

    async fn bulk_revert(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::BulkRevertRequest>,
    ) -> GrpcResult<pb::BulkRevertResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.bulk_revert_impl(r).await?))
            })
            .await?,
        )
    }

    async fn bisect(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::BisectRequest>,
    ) -> GrpcResult<pb::BisectResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.bisect_impl(r).await?))
            })
            .await?,
        )
    }

    async fn get_trunk_health(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetTrunkHealthRequest>,
    ) -> GrpcResult<pb::GetTrunkHealthResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_trunk_health_impl(request).await?))
    }

    async fn list_incidents(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListIncidentsRequest>,
    ) -> GrpcResult<pb::ListIncidentsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_incidents_impl(request).await?))
    }

    async fn freeze_trunk(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::FreezeTrunkRequest>,
    ) -> GrpcResult<pb::FreezeTrunkResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.freeze_trunk_impl(r).await?))
            })
            .await?,
        )
    }

    async fn unfreeze_trunk(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::UnfreezeTrunkRequest>,
    ) -> GrpcResult<pb::UnfreezeTrunkResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.unfreeze_trunk_impl(r).await?))
            })
            .await?,
        )
    }
}
