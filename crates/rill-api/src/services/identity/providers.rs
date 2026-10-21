//! An organization's identity providers.

use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::identity::IdentityProvider;
use rill_types::Timestamp;
use tonic::Request;

use crate::auth::{caller, require_org_admin};
use crate::error::{Error, Result};
use crate::state::AppState;

fn info(p: IdentityProvider) -> pb::IdentityProviderInfo {
    pb::IdentityProviderInfo {
        org: p.org,
        name: p.name,
        issuer: p.issuer,
        client_id: p.client_id,
        has_client_secret: p.client_secret.is_some(),
        cli_client_id: p.cli_client_id,
        scopes: p.scopes,
        username_claim: p.username_claim,
        groups_claim: p.groups_claim,
        auto_provision: p.auto_provision,
        created_at: timestamp_to_proto(p.created_at),
    }
}

pub async fn put(
    state: &AppState,
    request: Request<pb::PutIdentityProviderRequest>,
) -> Result<pb::PutIdentityProviderResponse> {
    let ctx = caller(&request)?;
    let spec = request
        .into_inner()
        .provider
        .ok_or_else(|| Error::invalid("provider is required"))?;
    require_org_admin(state, &ctx, &spec.org).await?;
    if state.stores.get_org(&spec.org).await?.is_none() {
        return Err(Error::NotFound(format!("org {:?}", spec.org)));
    }
    let issuer =
        url::Url::parse(&spec.issuer).map_err(|e| Error::invalid(format!("issuer: {e}")))?;
    let loopback = matches!(
        issuer.host(),
        Some(url::Host::Domain("localhost"))
            | Some(url::Host::Ipv4(std::net::Ipv4Addr::LOCALHOST))
            | Some(url::Host::Ipv6(std::net::Ipv6Addr::LOCALHOST))
    );
    if issuer.scheme() != "https" && !loopback && !state.dev_mode() {
        return Err(Error::invalid("the issuer must use https"));
    }
    if spec.name.is_empty() || spec.client_id.is_empty() {
        return Err(Error::invalid("name and client_id are required"));
    }
    let client_secret = match spec.client_secret {
        Some(secret) => Some(
            state
                .identity
                .keys()
                .key_service()
                .wrap(secret.as_bytes())
                .await?,
        ),
        None => None,
    };
    let scopes = if spec.scopes.is_empty() {
        vec!["openid".into(), "profile".into(), "email".into()]
    } else {
        spec.scopes
    };
    let provider = IdentityProvider {
        org: spec.org,
        name: spec.name,
        issuer: spec.issuer.trim_end_matches('/').to_string(),
        client_id: spec.client_id,
        client_secret,
        cli_client_id: spec.cli_client_id,
        scopes,
        username_claim: spec
            .username_claim
            .unwrap_or_else(|| "preferred_username".into()),
        groups_claim: spec.groups_claim,
        auto_provision: spec.auto_provision,
        created_at: Timestamp::now(),
    };
    state
        .identity
        .store()
        .put_provider(&provider, &ctx.principal)
        .await
        .map_err(|e| match e {
            rill_store::Error::Conflict(m) => Error::AlreadyExists(m),
            other => other.into(),
        })?;
    tracing::info!(by = %ctx.principal, org = %provider.org, provider = %provider.name, "identity provider saved");
    Ok(pb::PutIdentityProviderResponse {
        provider: Some(info(provider)),
    })
}

pub async fn list(
    state: &AppState,
    request: Request<pb::ListIdentityProvidersRequest>,
) -> Result<pb::ListIdentityProvidersResponse> {
    let ctx = caller(&request)?;
    let org = &request.get_ref().org;
    require_org_admin(state, &ctx, org).await?;
    let providers = state.identity.store().providers(org).await?;
    Ok(pb::ListIdentityProvidersResponse {
        providers: providers.into_iter().map(info).collect(),
    })
}

pub async fn delete(
    state: &AppState,
    request: Request<pb::DeleteIdentityProviderRequest>,
) -> Result<pb::DeleteIdentityProviderResponse> {
    let ctx = caller(&request)?;
    let req = request.get_ref();
    require_org_admin(state, &ctx, &req.org).await?;
    let deleted = state
        .identity
        .store()
        .delete_provider(&req.org, &req.name, &ctx.principal)
        .await?;
    Ok(pb::DeleteIdentityProviderResponse { deleted })
}
