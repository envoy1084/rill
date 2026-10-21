//! The identity service: token keys, revocation and sessions.

pub mod agents;
mod keys;
mod kill;
mod providers;
mod revoke;
pub mod scim_credentials;
mod users;
mod workloads;

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

    async fn put_identity_provider(
        &self,
        request: Request<pb::PutIdentityProviderRequest>,
    ) -> GrpcResult<pb::PutIdentityProviderResponse> {
        idem::once(&self.state, request, |r| async move {
            providers::put(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn list_identity_providers(
        &self,
        request: Request<pb::ListIdentityProvidersRequest>,
    ) -> GrpcResult<pb::ListIdentityProvidersResponse> {
        providers::list(&self.state, request)
            .await
            .map(Response::new)
            .map_err(Status::from)
    }

    async fn delete_identity_provider(
        &self,
        request: Request<pb::DeleteIdentityProviderRequest>,
    ) -> GrpcResult<pb::DeleteIdentityProviderResponse> {
        idem::once(&self.state, request, |r| async move {
            providers::delete(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn list_users(
        &self,
        request: Request<pb::ListUsersRequest>,
    ) -> GrpcResult<pb::ListUsersResponse> {
        users::list(&self.state, request)
            .await
            .map(Response::new)
            .map_err(Status::from)
    }

    async fn update_user(
        &self,
        request: Request<pb::UpdateUserRequest>,
    ) -> GrpcResult<pb::UpdateUserResponse> {
        idem::once(&self.state, request, |r| async move {
            users::update(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn register_agent(
        &self,
        request: Request<pb::RegisterAgentRequest>,
    ) -> GrpcResult<pb::RegisterAgentResponse> {
        idem::once(&self.state, request, |r| async move {
            agents::register(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn update_agent(
        &self,
        request: Request<pb::UpdateAgentRequest>,
    ) -> GrpcResult<pb::UpdateAgentResponse> {
        idem::once(&self.state, request, |r| async move {
            agents::update(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn list_agents(
        &self,
        request: Request<pb::ListAgentsRequest>,
    ) -> GrpcResult<pb::ListAgentsResponse> {
        agents::list(&self.state, request)
            .await
            .map(Response::new)
            .map_err(Status::from)
    }

    async fn put_workload_issuer(
        &self,
        request: Request<pb::PutWorkloadIssuerRequest>,
    ) -> GrpcResult<pb::PutWorkloadIssuerResponse> {
        idem::once(&self.state, request, |r| async move {
            workloads::put(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn list_workload_issuers(
        &self,
        request: Request<pb::ListWorkloadIssuersRequest>,
    ) -> GrpcResult<pb::ListWorkloadIssuersResponse> {
        workloads::list(&self.state, request)
            .await
            .map(Response::new)
            .map_err(Status::from)
    }

    async fn delete_workload_issuer(
        &self,
        request: Request<pb::DeleteWorkloadIssuerRequest>,
    ) -> GrpcResult<pb::DeleteWorkloadIssuerResponse> {
        idem::once(&self.state, request, |r| async move {
            workloads::delete(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn kill_switch(
        &self,
        request: Request<pb::KillSwitchRequest>,
    ) -> GrpcResult<pb::KillSwitchResponse> {
        idem::once(&self.state, request, |r| async move {
            kill::kill(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn create_scim_credential(
        &self,
        request: Request<pb::CreateScimCredentialRequest>,
    ) -> GrpcResult<pb::CreateScimCredentialResponse> {
        idem::once(&self.state, request, |r| async move {
            scim_credentials::create(&self.state, r)
                .await
                .map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn list_scim_credentials(
        &self,
        request: Request<pb::ListScimCredentialsRequest>,
    ) -> GrpcResult<pb::ListScimCredentialsResponse> {
        scim_credentials::list(&self.state, request)
            .await
            .map(Response::new)
            .map_err(Status::from)
    }

    async fn revoke_scim_credential(
        &self,
        request: Request<pb::RevokeScimCredentialRequest>,
    ) -> GrpcResult<pb::RevokeScimCredentialResponse> {
        idem::once(&self.state, request, |r| async move {
            scim_credentials::revoke(&self.state, r)
                .await
                .map(Response::new)
        })
        .await
        .map_err(Status::from)
    }
}
