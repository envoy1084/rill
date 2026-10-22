//! The `AccessService` impl: dispatches each RPC to its handler.

use std::sync::Arc;

use connectrpc::{RequestContext, Response, ServiceRequest};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::AccessService;

use super::{queries, relations};
use crate::services::common::GrpcResult;
use crate::services::Call;
use rill_runtime::state::AppState;

/// The `AccessService`.
#[derive(Clone)]
pub struct AccessSvc {
    pub(super) state: Arc<AppState>,
}

impl AccessSvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        AccessSvc { state }
    }
}

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl AccessService for AccessSvc {
    async fn create_relation(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::CreateRelationRequest>,
    ) -> GrpcResult<pb::CreateRelationResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(relations::create(&self.state, r).await?))
            })
            .await?,
        )
    }

    async fn delete_relation(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::DeleteRelationRequest>,
    ) -> GrpcResult<pb::DeleteRelationResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(relations::delete(&self.state, r).await?))
            })
            .await?,
        )
    }

    async fn list_relations(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListRelationsRequest>,
    ) -> GrpcResult<pb::ListRelationsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(relations::list(&self.state, request).await?))
    }

    async fn check_access(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::CheckAccessRequest>,
    ) -> GrpcResult<pb::CheckAccessResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(queries::check(&self.state, request).await?))
    }

    async fn expand_access(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ExpandAccessRequest>,
    ) -> GrpcResult<pb::ExpandAccessResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(queries::expand(&self.state, request).await?))
    }

    async fn lookup_paths(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::LookupPathsRequest>,
    ) -> GrpcResult<pb::LookupPathsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(
            queries::lookup_paths(&self.state, request).await?,
        ))
    }
}
