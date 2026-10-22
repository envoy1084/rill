//! `rill.v1.AccessService`: relationship tuples and questions about them.

mod queries;
mod relations;
mod subjects;

use std::sync::Arc;

use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::access_service_server::AccessService;
use tonic::{Request, Response, Status};

use super::idem;
use crate::state::AppState;

type GrpcResult<T> = std::result::Result<Response<T>, Status>;

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

#[tonic::async_trait]
impl AccessService for AccessSvc {
    async fn add_relation(
        &self,
        request: Request<pb::AddRelationRequest>,
    ) -> GrpcResult<pb::AddRelationResponse> {
        idem::once(&self.state, request, |r| async move {
            relations::add(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn remove_relation(
        &self,
        request: Request<pb::RemoveRelationRequest>,
    ) -> GrpcResult<pb::RemoveRelationResponse> {
        idem::once(&self.state, request, |r| async move {
            relations::remove(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn list_relations(
        &self,
        request: Request<pb::ListRelationsRequest>,
    ) -> GrpcResult<pb::ListRelationsResponse> {
        relations::list(&self.state, request)
            .await
            .map(Response::new)
            .map_err(Status::from)
    }

    async fn check(&self, request: Request<pb::CheckRequest>) -> GrpcResult<pb::CheckResponse> {
        queries::check(&self.state, request)
            .await
            .map(Response::new)
            .map_err(Status::from)
    }

    async fn expand(&self, request: Request<pb::ExpandRequest>) -> GrpcResult<pb::ExpandResponse> {
        queries::expand(&self.state, request)
            .await
            .map(Response::new)
            .map_err(Status::from)
    }

    async fn lookup_paths(
        &self,
        request: Request<pb::LookupPathsRequest>,
    ) -> GrpcResult<pb::LookupPathsResponse> {
        queries::lookup_paths(&self.state, request)
            .await
            .map(Response::new)
            .map_err(Status::from)
    }
}
