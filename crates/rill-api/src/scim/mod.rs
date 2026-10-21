//! The SCIM 2.0 endpoint each organization's provisioning system calls:
//! `/scim/v2/<org>/Users`, `/Groups` and the discovery resources. Callers
//! present a SCIM credential of that org (`scim_<id>.<secret>`).

mod discovery;
mod resources;

use std::sync::Arc;

use axum::extract::{Path, Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use rill_identity::scim::{Scim, ScimError};
use serde_json::Value;

use crate::services::identity::scim_credentials::{secret_hash, TOKEN_PREFIX};
use crate::state::AppState;

/// The SCIM routes.
pub fn router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/scim/v2/{org}/ServiceProviderConfig",
            get(discovery::service_provider_config),
        )
        .route(
            "/scim/v2/{org}/ResourceTypes",
            get(discovery::resource_types),
        )
        .route("/scim/v2/{org}/Schemas", get(discovery::schemas))
        .route(
            "/scim/v2/{org}/Users",
            get(resources::list_users).post(resources::create_user),
        )
        .route(
            "/scim/v2/{org}/Users/{id}",
            get(resources::get_user)
                .put(resources::replace_user)
                .patch(resources::patch_user)
                .delete(resources::delete_user),
        )
        .route(
            "/scim/v2/{org}/Groups",
            get(resources::list_groups).post(resources::create_group),
        )
        .route(
            "/scim/v2/{org}/Groups/{id}",
            get(resources::get_group)
                .put(resources::replace_group)
                .patch(resources::patch_group)
                .delete(resources::delete_group),
        )
        .layer(middleware::from_fn_with_state(
            state,
            require_scim_credential,
        ))
}

/// A SCIM JSON response.
pub fn scim_json(status: StatusCode, body: Value) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/scim+json")],
        Json(body),
    )
        .into_response()
}

pub fn scim_error(e: ScimError) -> Response {
    if e.status >= 500 {
        tracing::error!(error = %e.detail, "SCIM request failed");
    }
    let status = StatusCode::from_u16(e.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    scim_json(status, e.body())
}

/// The SCIM service of `org`.
pub fn scim_for(state: &AppState, org: &str) -> Scim {
    let base = match &state.config.identity.public_url {
        Some(url) => url.trim_end_matches('/').to_string(),
        None => format!("http://{}", state.config.listen_http),
    };
    Scim::new(
        state.identity.store().clone(),
        org,
        format!("{base}/scim/v2/{org}"),
    )
}

/// Refuse unless the request carries a live SCIM credential of the org in
/// its path.
async fn require_scim_credential(
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
