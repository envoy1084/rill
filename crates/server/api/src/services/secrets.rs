//! `rill.v1.SecretService`: org secrets for verify plans, set and listed
//! by org administrators; values are never returned.

use std::sync::Arc;

use rill_exec::remote::secrets::{SecretInfo, SecretScope};
use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::SecretService;
use rill_types::identity::IdentityChange;

use super::idem;
use crate::auth::{caller, require_org_admin};
use crate::services::Call;
use connectrpc::{ConnectError, RequestContext, Response, ServiceRequest};
use rill_runtime::error::Result;
use rill_runtime::state::AppState;

type GrpcResult<T> = std::result::Result<Response<T>, ConnectError>;

/// The secret service.
#[derive(Clone)]
pub struct SecretSvc {
    state: Arc<AppState>,
}

impl SecretSvc {
    pub fn new(state: Arc<AppState>) -> Self {
        SecretSvc { state }
    }
}

fn org_of(org: &str) -> String {
    if org.is_empty() {
        rill_store::DEFAULT_ORG.to_string()
    } else {
        org.to_string()
    }
}

fn info_proto(info: SecretInfo) -> pb::SecretInfo {
    pb::SecretInfo {
        name: info.name,
        repos: info.scope.repos,
        trunk_only: info.scope.trunk_only,
        updated_at: timestamp_to_proto(info.updated_at),
        updated_by: pb::Principal::from(info.updated_by).into(),
    }
}

impl SecretSvc {
    async fn put(&self, request: Call<pb::PutSecretRequest>) -> Result<pb::PutSecretResponse> {
        let ctx = caller(&request)?;
        let req = request.into_inner();
        let org = org_of(&req.org);
        require_org_admin(&self.state, &ctx, &org).await?;
        rill_exec::remote::secrets::validate_secret_name(&req.name)?;
        let log = self.state.stores.identity().log(
            IdentityChange::SecretChanged {
                org: org.clone(),
                name: req.name.clone(),
                deleted: false,
            },
            &ctx.principal,
        )?;
        let info = self
            .state
            .exec
            .put_secret(
                &org,
                &req.name,
                &req.value,
                SecretScope {
                    repos: req.repos,
                    trunk_only: req.trunk_only,
                },
                &ctx.principal,
                log.to_vec(),
            )
            .await?;
        tracing::info!(by = %ctx.principal, org, name = %info.name, "secret set");
        Ok(pb::PutSecretResponse {
            secret: info_proto(info).into(),
        })
    }

    async fn delete(
        &self,
        request: Call<pb::DeleteSecretRequest>,
    ) -> Result<pb::DeleteSecretResponse> {
        let ctx = caller(&request)?;
        let req = request.into_inner();
        let org = org_of(&req.org);
        require_org_admin(&self.state, &ctx, &org).await?;
        let log = self.state.stores.identity().log(
            IdentityChange::SecretChanged {
                org: org.clone(),
                name: req.name.clone(),
                deleted: true,
            },
            &ctx.principal,
        )?;
        let deleted = self
            .state
            .exec
            .delete_secret(&org, &req.name, log.to_vec())
            .await?;
        tracing::info!(by = %ctx.principal, org, name = %req.name, deleted, "secret deleted");
        Ok(pb::DeleteSecretResponse { deleted })
    }

    async fn list(&self, request: Call<pb::ListSecretsRequest>) -> Result<pb::ListSecretsResponse> {
        let ctx = caller(&request)?;
        let org = org_of(&request.get_ref().org);
        require_org_admin(&self.state, &ctx, &org).await?;
        let secrets = self.state.exec.list_secrets(&org).await?;
        Ok(pb::ListSecretsResponse {
            secrets: secrets.into_iter().map(info_proto).collect(),
        })
    }
}

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl SecretService for SecretSvc {
    // Not through `idem`: it keeps a hash of the request, which would
    // hold the value. Setting a secret again is harmless anyway.
    async fn put_secret(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::PutSecretRequest>,
    ) -> GrpcResult<pb::PutSecretResponse> {
        let request = Call::new(ctx, request);
        self.put(request)
            .await
            .map(Response::new)
            .map_err(ConnectError::from)
    }

    async fn delete_secret(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::DeleteSecretRequest>,
    ) -> GrpcResult<pb::DeleteSecretResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            self.delete(r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn list_secrets(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListSecretsRequest>,
    ) -> GrpcResult<pb::ListSecretsResponse> {
        let request = Call::new(ctx, request);
        self.list(request)
            .await
            .map(Response::new)
            .map_err(ConnectError::from)
    }
}
