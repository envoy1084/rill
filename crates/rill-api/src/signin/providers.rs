//! `GET /auth/providers?org=`: where a client can sign in, without
//! credentials.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::objects::error_response;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_org")]
    org: String,
}

fn default_org() -> String {
    rill_store::DEFAULT_ORG.to_string()
}

pub async fn list(State(state): State<Arc<AppState>>, Query(q): Query<ListQuery>) -> Response {
    let providers = match state.identity.store().providers(&q.org).await {
        Ok(p) => p,
        Err(e) => return error_response(crate::error::Error::from(e).into()),
    };
    let mut out = Vec::new();
    for p in providers {
        // Discovery is best effort: a provider that is down is still listed.
        let device = state
            .identity
            .issuers()
            .metadata(&p.issuer)
            .await
            .ok()
            .and_then(|m| m.device_authorization_endpoint);
        out.push(json!({
            "org": p.org,
            "name": p.name,
            "issuer": p.issuer,
            "cli_client_id": p.cli_client_id,
            "scopes": p.scopes,
            "device_authorization_endpoint": device,
        }));
    }
    Json(json!({ "providers": out })).into_response()
}
