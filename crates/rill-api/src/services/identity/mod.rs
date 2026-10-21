//! The identity service: token keys, revocation and sessions.

mod keys;
mod revoke;

use std::sync::Arc;

use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::identity_service_server::IdentityService;
use tonic::{Request, Response, Status};

use super::idem;
use crate::state::AppState;

type GrpcResult<T> = std::result::Result<Response<T>, Status>;

/// The identity service.
#[derive(Clone)]
pub struct IdentitySvc {
    state: Arc<AppState>,
}

impl IdentitySvc {
    pub fn new(state: Arc<AppState>) -> Self {
        IdentitySvc { state }
    }
}

#[tonic::async_trait]
impl IdentityService for IdentitySvc {
    async fn revoke_token(
        &self,
        request: Request<pb::RevokeTokenRequest>,
    ) -> GrpcResult<pb::RevokeTokenResponse> {
        idem::once(&self.state, request, |r| async move {
            revoke::token(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn revoke_session(
        &self,
        request: Request<pb::RevokeSessionRequest>,
    ) -> GrpcResult<pb::RevokeSessionResponse> {
        idem::once(&self.state, request, |r| async move {
            revoke::session(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn rotate_token_keys(
        &self,
        request: Request<pb::RotateTokenKeysRequest>,
    ) -> GrpcResult<pb::RotateTokenKeysResponse> {
        idem::once(&self.state, request, |r| async move {
            keys::rotate(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn list_token_keys(
        &self,
        request: Request<pb::ListTokenKeysRequest>,
    ) -> GrpcResult<pb::ListTokenKeysResponse> {
        keys::list(&self.state, request)
            .await
            .map(Response::new)
            .map_err(Status::from)
    }
}
