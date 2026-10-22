//! The `IntentService` impl: dispatches each RPC to its handler.

use std::sync::Arc;

use connectrpc::{RequestContext, Response, ServiceRequest};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::IntentService;

use crate::services::common::GrpcResult;
use crate::services::Call;
use rill_runtime::state::AppState;

/// The `IntentService`.
#[derive(Clone)]
pub struct IntentSvc {
    pub(super) state: Arc<AppState>,
}

impl IntentSvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        IntentSvc { state }
    }
}

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl IntentService for IntentSvc {
    async fn create_intent(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::CreateIntentRequest>,
    ) -> GrpcResult<pb::CreateIntentResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.create_intent_impl(r).await?))
            })
            .await?,
        )
    }

    async fn get_intent(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetIntentRequest>,
    ) -> GrpcResult<pb::GetIntentResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_intent_impl(request).await?))
    }

    async fn update_intent(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::UpdateIntentRequest>,
    ) -> GrpcResult<pb::UpdateIntentResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.update_intent_impl(r).await?))
            })
            .await?,
        )
    }

    async fn list_intents(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListIntentsRequest>,
    ) -> GrpcResult<pb::ListIntentsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_intents_impl(request).await?))
    }

    async fn link_change(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::LinkChangeRequest>,
    ) -> GrpcResult<pb::LinkChangeResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.link_change_impl(r).await?))
            })
            .await?,
        )
    }

    async fn find_similar_intents(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::FindSimilarIntentsRequest>,
    ) -> GrpcResult<pb::FindSimilarIntentsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(
            self.find_similar_intents_impl(request).await?,
        ))
    }
}
