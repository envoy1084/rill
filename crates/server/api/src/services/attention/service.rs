//! The `AttentionService` impl: dispatches each RPC to its handler.

use std::sync::Arc;

use connectrpc::{RequestContext, Response, ServiceRequest};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::AttentionService;

use crate::services::common::GrpcResult;
use crate::services::Call;
use rill_runtime::state::AppState;

/// The `AttentionService`.
#[derive(Clone)]
pub struct AttentionSvc {
    pub(super) state: Arc<AppState>,
}

impl AttentionSvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        AttentionSvc { state }
    }
}

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl AttentionService for AttentionSvc {
    async fn list_inbox(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListInboxRequest>,
    ) -> GrpcResult<pb::ListInboxResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_inbox_impl(request).await?))
    }

    async fn list_audits(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListAuditsRequest>,
    ) -> GrpcResult<pb::ListAuditsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_audits_impl(request).await?))
    }

    async fn record_audit(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::RecordAuditRequest>,
    ) -> GrpcResult<pb::RecordAuditResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.record_audit_impl(r).await?))
            })
            .await?,
        )
    }

    async fn list_similar_changes(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListSimilarChangesRequest>,
    ) -> GrpcResult<pb::ListSimilarChangesResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(
            self.list_similar_changes_impl(request).await?,
        ))
    }

    async fn batch_vote(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::BatchVoteRequest>,
    ) -> GrpcResult<pb::BatchVoteResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.batch_vote_impl(r).await?))
            })
            .await?,
        )
    }

    async fn get_intent_digest(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetIntentDigestRequest>,
    ) -> GrpcResult<pb::GetIntentDigestResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_intent_digest_impl(request).await?))
    }
}
