//! The `IdentityService` gRPC impl: dispatches each RPC to its handler.

use std::sync::Arc;

use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::IdentityService;

use super::{agents, keys, kill, providers, revoke, scim_credentials, users, workloads};
use crate::services::common::GrpcResult;
use crate::services::idem;
use crate::services::Call;
use connectrpc::{ConnectError, RequestContext, Response, ServiceRequest};
use rill_runtime::state::AppState;

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

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl IdentityService for IdentitySvc {
    async fn revoke_token(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::RevokeTokenRequest>,
    ) -> GrpcResult<pb::RevokeTokenResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            revoke::token(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn revoke_session(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::RevokeSessionRequest>,
    ) -> GrpcResult<pb::RevokeSessionResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            revoke::session(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn rotate_token_keys(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::RotateTokenKeysRequest>,
    ) -> GrpcResult<pb::RotateTokenKeysResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            keys::rotate(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn list_token_keys(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListTokenKeysRequest>,
    ) -> GrpcResult<pb::ListTokenKeysResponse> {
        let request = Call::new(ctx, request);
        keys::list(&self.state, request)
            .await
            .map(Response::new)
            .map_err(ConnectError::from)
    }

    async fn put_identity_provider(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::PutIdentityProviderRequest>,
    ) -> GrpcResult<pb::PutIdentityProviderResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            providers::put(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn list_identity_providers(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListIdentityProvidersRequest>,
    ) -> GrpcResult<pb::ListIdentityProvidersResponse> {
        let request = Call::new(ctx, request);
        providers::list(&self.state, request)
            .await
            .map(Response::new)
            .map_err(ConnectError::from)
    }

    async fn delete_identity_provider(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::DeleteIdentityProviderRequest>,
    ) -> GrpcResult<pb::DeleteIdentityProviderResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            providers::delete(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn list_users(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListUsersRequest>,
    ) -> GrpcResult<pb::ListUsersResponse> {
        let request = Call::new(ctx, request);
        users::list(&self.state, request)
            .await
            .map(Response::new)
            .map_err(ConnectError::from)
    }

    async fn update_user(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::UpdateUserRequest>,
    ) -> GrpcResult<pb::UpdateUserResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            users::update(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn register_agent(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::RegisterAgentRequest>,
    ) -> GrpcResult<pb::RegisterAgentResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            agents::register(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn update_agent(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::UpdateAgentRequest>,
    ) -> GrpcResult<pb::UpdateAgentResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            agents::update(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn list_agents(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListAgentsRequest>,
    ) -> GrpcResult<pb::ListAgentsResponse> {
        let request = Call::new(ctx, request);
        agents::list(&self.state, request)
            .await
            .map(Response::new)
            .map_err(ConnectError::from)
    }

    async fn put_workload_issuer(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::PutWorkloadIssuerRequest>,
    ) -> GrpcResult<pb::PutWorkloadIssuerResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            workloads::put(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn list_workload_issuers(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListWorkloadIssuersRequest>,
    ) -> GrpcResult<pb::ListWorkloadIssuersResponse> {
        let request = Call::new(ctx, request);
        workloads::list(&self.state, request)
            .await
            .map(Response::new)
            .map_err(ConnectError::from)
    }

    async fn delete_workload_issuer(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::DeleteWorkloadIssuerRequest>,
    ) -> GrpcResult<pb::DeleteWorkloadIssuerResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            workloads::delete(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn kill_switch(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::KillSwitchRequest>,
    ) -> GrpcResult<pb::KillSwitchResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            kill::kill(&self.state, r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn create_scim_credential(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::CreateScimCredentialRequest>,
    ) -> GrpcResult<pb::CreateScimCredentialResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            scim_credentials::create(&self.state, r)
                .await
                .map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn list_scim_credentials(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListScimCredentialsRequest>,
    ) -> GrpcResult<pb::ListScimCredentialsResponse> {
        let request = Call::new(ctx, request);
        scim_credentials::list(&self.state, request)
            .await
            .map(Response::new)
            .map_err(ConnectError::from)
    }

    async fn revoke_scim_credential(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::RevokeScimCredentialRequest>,
    ) -> GrpcResult<pb::RevokeScimCredentialResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            scim_credentials::revoke(&self.state, r)
                .await
                .map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }
}
