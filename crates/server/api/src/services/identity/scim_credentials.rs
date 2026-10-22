//! SCIM bearer credentials.

use base64::Engine as _;
use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::identity::ScimCredential;
use rill_types::Timestamp;

use crate::auth::{caller, require_org_admin};
use crate::listing::Page;
use crate::services::{names, Call};
use rill_proto::names::SCIM_CREDENTIAL;
use rill_runtime::error::{Error, Result};
use rill_runtime::state::AppState;

/// The prefix of SCIM bearer tokens: `scim_<id>.<secret>`.
pub const TOKEN_PREFIX: &str = "scim_";

/// The stored hash of a SCIM secret.
pub fn secret_hash(secret: &str) -> [u8; 32] {
    *blake3::keyed_hash(
        &blake3::derive_key("rill scim credential", b""),
        secret.as_bytes(),
    )
    .as_bytes()
}

fn info(c: &ScimCredential) -> pb::ScimCredential {
    pb::ScimCredential {
        name: names::org_resource(&SCIM_CREDENTIAL, &c.org, &c.id),
        org: c.org.clone(),
        id: c.id.clone(),
        created_by: pb::Principal::from(&c.created_by).into(),
        created_at: timestamp_to_proto(c.created_at),
        revoked_at: c.revoked_at.map(timestamp_to_proto),
    }
}

pub async fn create(
    state: &AppState,
    request: Call<pb::CreateScimCredentialRequest>,
) -> Result<pb::CreateScimCredentialResponse> {
    let ctx = caller(&request)?;
    let org = names::org(&request.get_ref().parent)?;
    require_org_admin(state, &ctx, &org).await?;
    if state.stores.get_org(&org).await?.is_none() {
        return Err(Error::NotFound(format!("org {org:?}")));
    }
    let id = hex::encode(rand::random::<[u8; 8]>());
    let secret =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>());
    let credential = ScimCredential {
        org,
        id: id.clone(),
        secret_hash: secret_hash(&secret),
        created_by: ctx.principal.clone(),
        created_at: Timestamp::now(),
        revoked_at: None,
    };
    state
        .identity
        .store()
        .create_scim_credential(&credential, &ctx.principal)
        .await?;
    tracing::info!(by = %ctx.principal, org = %credential.org, credential = %id, "SCIM credential created");
    Ok(pb::CreateScimCredentialResponse {
        credential: info(&credential).into(),
        token: format!("{TOKEN_PREFIX}{id}.{secret}"),
    })
}

pub async fn list(
    state: &AppState,
    request: Call<pb::ListScimCredentialsRequest>,
) -> Result<pb::ListScimCredentialsResponse> {
    let ctx = caller(&request)?;
    let req = request.get_ref();
    let org = names::org(&req.parent)?;
    require_org_admin(state, &ctx, &org).await?;
    let page = Page::new(req.page_size, &req.page_token, &[&req.parent])?;
    let (credentials, next_page_token) =
        page.of(state.identity.store().scim_credentials(&org).await?)?;
    Ok(pb::ListScimCredentialsResponse {
        credentials: credentials.iter().map(info).collect(),
        next_page_token,
    })
}

pub async fn delete(
    state: &AppState,
    request: Call<pb::DeleteScimCredentialRequest>,
) -> Result<pb::DeleteScimCredentialResponse> {
    let ctx = caller(&request)?;
    let (org, id) = names::org_child(&SCIM_CREDENTIAL, &request.get_ref().name)?;
    require_org_admin(state, &ctx, &org).await?;
    let store = state.identity.store();
    match store.scim_credential(&id).await? {
        Some(c) if c.org == org => store.revoke_scim_credential(&id, &ctx.principal).await?,
        _ => return Err(Error::NotFound(format!("SCIM credential {id}"))),
    }
    tracing::info!(by = %ctx.principal, org = %org, credential = %id, "SCIM credential revoked");
    Ok(pb::DeleteScimCredentialResponse {})
}
