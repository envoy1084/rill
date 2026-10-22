//! The `ProtectionService` impl: dispatches each RPC to its handler.

use std::sync::Arc;

use connectrpc::{RequestContext, Response, ServiceRequest};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::ProtectionService;

use crate::services::common::GrpcResult;
use crate::services::Call;
use rill_runtime::state::AppState;

/// The `ProtectionService`.
#[derive(Clone)]
pub struct ProtectionSvc {
    pub(super) state: Arc<AppState>,
}

impl ProtectionSvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        ProtectionSvc { state }
    }
}

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl ProtectionService for ProtectionSvc {
    async fn create_redaction(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::CreateRedactionRequest>,
    ) -> GrpcResult<pb::CreateRedactionResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.create_redaction_impl(r).await?))
            })
            .await?,
        )
    }

    async fn approve_redaction(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ApproveRedactionRequest>,
    ) -> GrpcResult<pb::ApproveRedactionResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.approve_redaction_impl(r).await?))
            })
            .await?,
        )
    }

    async fn list_redactions(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListRedactionsRequest>,
    ) -> GrpcResult<pb::ListRedactionsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_redactions_impl(request).await?))
    }

    async fn release_version(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ReleaseVersionRequest>,
    ) -> GrpcResult<pb::ReleaseVersionResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.release_version_impl(r).await?))
            })
            .await?,
        )
    }

    async fn rotate_data_keys(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::RotateDataKeysRequest>,
    ) -> GrpcResult<pb::RotateDataKeysResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.rotate_data_keys_impl(request).await?))
    }
}
