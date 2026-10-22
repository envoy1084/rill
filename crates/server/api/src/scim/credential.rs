//! The middleware that admits only an org's live SCIM credentials.

use std::sync::Arc;

use axum::extract::{Path, Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use rill_identity::scim::ScimError;
use rill_runtime::state::AppState;

use super::{scim_error, scim_json};
use crate::services::identity::scim_credentials::{secret_hash, TOKEN_PREFIX};

/// Refuse unless the request carries a live SCIM credential of the org in
/// its path.
pub(super) async fn require_scim_credential(
    State(state): State<Arc<AppState>>,
    Path(params): Path<std::collections::HashMap<String, String>>,
    request: Request,
    next: Next,
) -> Response {
    let unauthorized = |detail: &str| {
        scim_json(
            StatusCode::UNAUTHORIZED,
            ScimError {
                status: 401,
                scim_type: None,
                detail: detail.into(),
            }
            .body(),
        )
    };
    let Some(org) = params.get("org") else {
        return unauthorized("no organization in the path");
    };
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(crate::auth::bearer_token)
        .and_then(|t| t.strip_prefix(TOKEN_PREFIX))
        .and_then(|t| t.split_once('.'))
        .map(|(id, secret)| (id.to_string(), secret.to_string()));
    let Some((id, secret)) = token else {
        return unauthorized("a SCIM bearer token is required");
    };
    let credential = match state.identity.store().scim_credential(&id).await {
        Ok(Some(c)) => c,
        Ok(None) => return unauthorized("unknown SCIM credential"),
        Err(e) => return scim_error(ScimError::internal(e)),
    };
    let matches =
        blake3::Hash::from(credential.secret_hash) == blake3::Hash::from(secret_hash(&secret));
    if !matches || credential.revoked_at.is_some() || credential.org != *org {
        return unauthorized("the SCIM credential is not valid for this organization");
    }
    next.run(request).await
}
