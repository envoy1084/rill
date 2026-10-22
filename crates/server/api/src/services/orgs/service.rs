//! The `OrgService` impl: dispatches each RPC to its handler.

use std::sync::Arc;

use connectrpc::{RequestContext, Response, ServiceRequest};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::OrgService;

use crate::services::common::GrpcResult;
use crate::services::Call;
use rill_runtime::state::AppState;

/// The `OrgService`.
#[derive(Clone)]
pub struct OrgSvc {
    pub(super) state: Arc<AppState>,
}

impl OrgSvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        OrgSvc { state }
    }
}

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl OrgService for OrgSvc {
    async fn create_org(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::CreateOrgRequest>,
    ) -> GrpcResult<pb::CreateOrgResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.create_org_impl(r).await?))
            })
            .await?,
        )
    }

    async fn get_org(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetOrgRequest>,
    ) -> GrpcResult<pb::GetOrgResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_org_impl(request).await?))
    }

    async fn list_orgs(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListOrgsRequest>,
    ) -> GrpcResult<pb::ListOrgsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_orgs_impl(request).await?))
    }
}
