//! The `AccessService` gRPC impl: dispatches each RPC to its handler.

use std::sync::Arc;

use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::AccessService;

use super::{queries, relations};
use crate::services::common::GrpcResult;
use crate::services::idem;
use crate::services::Call;
use connectrpc::{ConnectError, RequestContext, Response, ServiceRequest};
use rill_runtime::state::AppState;

/// The access service.
#[derive(Clone)]
pub struct AccessSvc {
    state: Arc<AppState>,
}

impl AccessSvc {
    pub fn new(state: Arc<AppState>) -> Self {
        AccessSvc { state }
    }
}

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl AccessService for AccessSvc {
    async fn add_relation(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::AddRelationRequest>,
    ) -> GrpcResult<pb::AddRelationResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            relations::add(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn remove_relation(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::RemoveRelationRequest>,
    ) -> GrpcResult<pb::RemoveRelationResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            relations::remove(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn list_relations(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListRelationsRequest>,
    ) -> GrpcResult<pb::ListRelationsResponse> {
        let request = Call::new(ctx, request);
        relations::list(&self.state, request)
            .await
            .map(Response::new)
            .map_err(ConnectError::from)
    }

    async fn check(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::CheckRequest>,
    ) -> GrpcResult<pb::CheckResponse> {
        let request = Call::new(ctx, request);
        queries::check(&self.state, request)
            .await
            .map(Response::new)
            .map_err(ConnectError::from)
    }

    async fn expand(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ExpandRequest>,
    ) -> GrpcResult<pb::ExpandResponse> {
        let request = Call::new(ctx, request);
        queries::expand(&self.state, request)
            .await
            .map(Response::new)
            .map_err(ConnectError::from)
    }

    async fn lookup_paths(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::LookupPathsRequest>,
    ) -> GrpcResult<pb::LookupPathsResponse> {
        let request = Call::new(ctx, request);
        queries::lookup_paths(&self.state, request)
            .await
            .map(Response::new)
            .map_err(ConnectError::from)
    }
}
