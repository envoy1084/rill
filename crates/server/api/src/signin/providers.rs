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
    match super::flows::sign_in_providers(&state, &q.org).await {
        Ok(providers) => {
            let out: Vec<_> = providers
                .into_iter()
                .map(|p| {
                    json!({
                        "org": p.org,
                        "name": p.name,
                        "issuer": p.issuer,
                        "cli_client_id": p.cli_client_id,
                        "scopes": p.scopes,
                        "device_authorization_endpoint": p.device_authorization_endpoint,
                        "token_endpoint": p.token_endpoint,
                    })
                })
                .collect();
            Json(json!({ "providers": out })).into_response()
        }
        Err(e) => error_response(e.into()),
    }
}
