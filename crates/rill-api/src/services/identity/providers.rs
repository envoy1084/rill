//! An organization's identity providers.

use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::identity::{IdentityProvider, SamlSettings};
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
        saml: p.saml.is_some(),
        saml_sp_entity_id: p.saml.map(|s| s.sp_entity_id),
    }
}

/// The settings of a SAML provider, checked as sign-in will use them.
#[cfg(feature = "saml")]
fn saml_settings(
    state: &AppState,
    spec: &pb::IdentityProviderSpec,
    metadata: String,
) -> Result<(String, SamlSettings)> {
    let settings = SamlSettings {
        idp_metadata: metadata,
        sp_entity_id: spec.saml_sp_entity_id.clone().unwrap_or_else(|| {
            format!(
                "{}/auth/saml/{}/{}",
                crate::signin::public_url(state),
                spec.org,
                spec.name
            )
        }),
    };
    let sp = rill_identity::saml::SamlProvider::new(&settings, "")
        .map_err(|e| Error::invalid(e.to_string()))?;
    Ok((sp.entity_id().to_string(), settings))
}

#[cfg(not(feature = "saml"))]
fn saml_settings(
    _: &AppState,
    _: &pb::IdentityProviderSpec,
    _: String,
) -> Result<(String, SamlSettings)> {
    Err(Error::invalid("this server was built without SAML support"))
}

/// An OIDC issuer must use https outside development.
fn check_issuer(state: &AppState, issuer: &str) -> Result<()> {
    let url = url::Url::parse(issuer).map_err(|e| Error::invalid(format!("issuer: {e}")))?;
    let loopback = matches!(
        url.host(),
        Some(url::Host::Domain("localhost"))
            | Some(url::Host::Ipv4(std::net::Ipv4Addr::LOCALHOST))
            | Some(url::Host::Ipv6(std::net::Ipv6Addr::LOCALHOST))
    );
    if url.scheme() != "https" && !loopback && !state.dev_mode() {
        return Err(Error::invalid("the issuer must use https"));
    }
    Ok(())
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
    if spec.name.is_empty() {
        return Err(Error::invalid("name is required"));
    }
    let (issuer, client_id, saml) = match spec.saml_metadata.clone() {
        Some(metadata) => {
            let (entity_id, settings) = saml_settings(state, &spec, metadata)?;
            let sp_entity_id = settings.sp_entity_id.clone();
            (entity_id, sp_entity_id, Some(settings))
        }
        None => {
            check_issuer(state, &spec.issuer)?;
            if spec.client_id.is_empty() {
                return Err(Error::invalid("client_id is required"));
            }
            let issuer = spec.issuer.trim_end_matches('/').to_string();
            (issuer, spec.client_id.clone(), None)
        }
    };
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
    let scopes = if spec.scopes.is_empty() && saml.is_none() {
        vec!["openid".into(), "profile".into(), "email".into()]
    } else {
        spec.scopes
    };
    let provider = IdentityProvider {
        org: spec.org,
        name: spec.name,
        issuer,
        client_id,
        client_secret,
        cli_client_id: spec.cli_client_id,
        scopes,
        username_claim: spec.username_claim.unwrap_or_else(|| {
            if saml.is_some() {
                "sub"
            } else {
                "preferred_username"
            }
            .into()
        }),
        groups_claim: spec.groups_claim,
        auto_provision: spec.auto_provision,
        saml,
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
