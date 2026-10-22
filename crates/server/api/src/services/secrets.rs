//! `rill.v1.SecretService`: org secrets for verify plans, set and listed
//! by org administrators; values are never returned.

use std::sync::Arc;

use rill_exec::remote::secrets::{SecretInfo, SecretScope};
use rill_proto::convert::timestamp_to_proto;
use rill_proto::names::SECRET;
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::SecretService;
use rill_types::identity::IdentityChange;

use super::{idem, names};
use crate::auth::{caller, require_org_admin};
use crate::listing::Page;
use crate::services::Call;
use connectrpc::{ConnectError, RequestContext, Response, ServiceRequest};
use rill_runtime::error::{Error, Result};
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

fn secret_proto(org: &str, info: SecretInfo) -> pb::Secret {
    pb::Secret {
        name: names::org_resource(&SECRET, org, &info.name),
        repos: info.scope.repos,
        trunk_only: info.scope.trunk_only,
        updated_at: timestamp_to_proto(info.updated_at),
        updated_by: pb::Principal::from(info.updated_by).into(),
    }
}

impl SecretSvc {
    async fn update(
        &self,
        request: Call<pb::UpdateSecretRequest>,
    ) -> Result<pb::UpdateSecretResponse> {
        let ctx = caller(&request)?;
        let req = request.into_inner();
        let given = req
            .secret
            .into_option()
            .ok_or(rill_proto::ConvertError::missing("secret"))?;
        let (org, name) = names::org_child(&SECRET, &given.name)?;
        require_org_admin(&self.state, &ctx, &org).await?;
        rill_exec::remote::secrets::validate_secret_name(&name)?;
        if !req.allow_missing {
            let exists = self
                .state
                .exec
                .list_secrets(&org)
                .await?
                .iter()
                .any(|s| s.name == name);
            if !exists {
                return Err(Error::not_found(format!(
                    "secret {name}; set allow_missing to create it"
                )));
            }
        }
        let log = self.state.stores.identity().log(
            IdentityChange::SecretChanged {
                org: org.clone(),
                name: name.clone(),
                deleted: false,
            },
            &ctx.principal,
        )?;
        let info = self
            .state
            .exec
            .put_secret(
                &org,
                &name,
                &req.value,
                SecretScope {
                    repos: given.repos,
                    trunk_only: given.trunk_only,
                },
                &ctx.principal,
                log.to_vec(),
            )
            .await?;
        tracing::info!(by = %ctx.principal, org, name = %info.name, "secret set");
        Ok(pb::UpdateSecretResponse {
            secret: secret_proto(&org, info).into(),
        })
    }

    async fn delete(
        &self,
        request: Call<pb::DeleteSecretRequest>,
    ) -> Result<pb::DeleteSecretResponse> {
        let ctx = caller(&request)?;
        let (org, name) = names::org_child(&SECRET, &request.get_ref().name)?;
        require_org_admin(&self.state, &ctx, &org).await?;
        let log = self.state.stores.identity().log(
            IdentityChange::SecretChanged {
                org: org.clone(),
                name: name.clone(),
                deleted: true,
            },
            &ctx.principal,
        )?;
        let deleted = self
            .state
            .exec
            .delete_secret(&org, &name, log.to_vec())
            .await?;
        if !deleted {
            return Err(Error::not_found(format!("secret {name}")));
        }
        tracing::info!(by = %ctx.principal, org, name = %name, "secret deleted");
        Ok(pb::DeleteSecretResponse {})
    }

    async fn list(&self, request: Call<pb::ListSecretsRequest>) -> Result<pb::ListSecretsResponse> {
        let ctx = caller(&request)?;
        let req = request.get_ref();
        let org = names::org(&req.parent)?;
        require_org_admin(&self.state, &ctx, &org).await?;
        let page = Page::new(req.page_size, &req.page_token, &[&req.parent])?;
        let secrets = self.state.exec.list_secrets(&org).await?;
        let (secrets, next_page_token) = page.of(secrets)?;
        Ok(pb::ListSecretsResponse {
            secrets: secrets.into_iter().map(|s| secret_proto(&org, s)).collect(),
            next_page_token,
        })
    }
}

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl SecretService for SecretSvc {
    // Not through `idem`: it keeps a hash of the request, which would
    // hold the value. Setting a secret again is harmless anyway.
    async fn update_secret(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::UpdateSecretRequest>,
    ) -> GrpcResult<pb::UpdateSecretResponse> {
        let request = Call::new(ctx, request);
        self.update(request)
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
