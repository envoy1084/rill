//! The `IdentityService` impl: dispatches each RPC to its handler.

use std::sync::Arc;

use connectrpc::{RequestContext, Response, ServiceRequest};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::IdentityService;

use super::{agents, keys, kill, providers, revoke, scim_credentials, users, workloads};
use crate::services::common::GrpcResult;
use crate::services::Call;
use rill_runtime::state::AppState;

/// The `IdentityService`.
#[derive(Clone)]
pub struct IdentitySvc {
    pub(super) state: Arc<AppState>,
}

impl IdentitySvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        IdentitySvc { state }
    }
}

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl IdentityService for IdentitySvc {
    async fn get_caller(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetCallerRequest>,
    ) -> GrpcResult<pb::GetCallerResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_caller_impl(request).await?))
    }

    async fn issue_token(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::IssueTokenRequest>,
    ) -> GrpcResult<pb::IssueTokenResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.issue_token_impl(r).await?))
            })
            .await?,
        )
    }

    async fn revoke_token(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::RevokeTokenRequest>,
    ) -> GrpcResult<pb::RevokeTokenResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(revoke::token(&self.state, r).await?))
            })
            .await?,
        )
    }

    async fn revoke_session(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::RevokeSessionRequest>,
    ) -> GrpcResult<pb::RevokeSessionResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(revoke::session(&self.state, r).await?))
            })
            .await?,
        )
    }

    async fn rotate_token_keys(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::RotateTokenKeysRequest>,
    ) -> GrpcResult<pb::RotateTokenKeysResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(keys::rotate(&self.state, request).await?))
    }

    async fn list_token_keys(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListTokenKeysRequest>,
    ) -> GrpcResult<pb::ListTokenKeysResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(keys::list(&self.state, request).await?))
    }

    async fn get_identity_provider(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetIdentityProviderRequest>,
    ) -> GrpcResult<pb::GetIdentityProviderResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(providers::get(&self.state, request).await?))
    }

    async fn list_identity_providers(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListIdentityProvidersRequest>,
    ) -> GrpcResult<pb::ListIdentityProvidersResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(providers::list(&self.state, request).await?))
    }

    async fn update_identity_provider(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::UpdateIdentityProviderRequest>,
    ) -> GrpcResult<pb::UpdateIdentityProviderResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(providers::update(&self.state, r).await?))
            })
            .await?,
        )
    }

    async fn delete_identity_provider(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::DeleteIdentityProviderRequest>,
    ) -> GrpcResult<pb::DeleteIdentityProviderResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(providers::delete(&self.state, r).await?))
            })
            .await?,
        )
    }

    async fn list_users(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListUsersRequest>,
    ) -> GrpcResult<pb::ListUsersResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(users::list(&self.state, request).await?))
    }

    async fn update_user(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::UpdateUserRequest>,
    ) -> GrpcResult<pb::UpdateUserResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(users::update(&self.state, r).await?))
            })
            .await?,
        )
    }

    async fn create_agent(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::CreateAgentRequest>,
    ) -> GrpcResult<pb::CreateAgentResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(agents::create(&self.state, r).await?))
            })
            .await?,
        )
    }

    async fn get_agent(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetAgentRequest>,
    ) -> GrpcResult<pb::GetAgentResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(agents::get(&self.state, request).await?))
    }

    async fn list_agents(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListAgentsRequest>,
    ) -> GrpcResult<pb::ListAgentsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(agents::list(&self.state, request).await?))
    }

    async fn update_agent(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::UpdateAgentRequest>,
    ) -> GrpcResult<pb::UpdateAgentResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(agents::update(&self.state, r).await?))
            })
            .await?,
        )
    }

    async fn list_workload_issuers(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListWorkloadIssuersRequest>,
    ) -> GrpcResult<pb::ListWorkloadIssuersResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(workloads::list(&self.state, request).await?))
    }

    async fn update_workload_issuer(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::UpdateWorkloadIssuerRequest>,
    ) -> GrpcResult<pb::UpdateWorkloadIssuerResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(workloads::update(&self.state, r).await?))
            })
            .await?,
        )
    }

    async fn delete_workload_issuer(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::DeleteWorkloadIssuerRequest>,
    ) -> GrpcResult<pb::DeleteWorkloadIssuerResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(workloads::delete(&self.state, r).await?))
            })
            .await?,
        )
    }

    async fn kill_switch(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::KillSwitchRequest>,
    ) -> GrpcResult<pb::KillSwitchResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(kill::kill(&self.state, request).await?))
    }

    async fn create_scim_credential(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::CreateScimCredentialRequest>,
    ) -> GrpcResult<pb::CreateScimCredentialResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(
                    scim_credentials::create(&self.state, r).await?,
                ))
            })
            .await?,
        )
    }

    async fn list_scim_credentials(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListScimCredentialsRequest>,
    ) -> GrpcResult<pb::ListScimCredentialsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(
            scim_credentials::list(&self.state, request).await?,
        ))
    }

    async fn delete_scim_credential(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::DeleteScimCredentialRequest>,
    ) -> GrpcResult<pb::DeleteScimCredentialResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(
                    scim_credentials::delete(&self.state, r).await?,
                ))
            })
            .await?,
        )
    }
}
