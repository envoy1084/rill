//! The `PolicyService` impl: dispatches each RPC to its handler.

use std::sync::Arc;

use connectrpc::{RequestContext, Response, ServiceRequest};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::PolicyService;

use crate::services::common::GrpcResult;
use crate::services::Call;
use rill_runtime::state::AppState;

/// The `PolicyService`.
#[derive(Clone)]
pub struct PolicySvc {
    pub(super) state: Arc<AppState>,
}

impl PolicySvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        PolicySvc { state }
    }
}

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl PolicyService for PolicySvc {
    async fn get_policy(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetPolicyRequest>,
    ) -> GrpcResult<pb::GetPolicyResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_policy_impl(request).await?))
    }

    async fn update_policy(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::UpdatePolicyRequest>,
    ) -> GrpcResult<pb::UpdatePolicyResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.update_policy_impl(r).await?))
            })
            .await?,
        )
    }
}
