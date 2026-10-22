//! The public keys cache tiers and auditors verify with.

use std::sync::Arc;

use axum::response::{IntoResponse, Response};
use rill_runtime::state::AppState;

/// The public keys derived from each token key: read grants verify with
/// the grant key (cache tiers), audit roots with the audit key (auditors).
/// Retired keys are listed with the time they stopped verifying grants.
pub(super) async fn public_keys(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> Response {
    let audit: std::collections::HashMap<u32, Vec<u8>> = state
        .identity
        .audit_keys()
        .into_iter()
        .map(|(id, key, _)| (id, key))
        .collect();
    let keys: Vec<_> = state
        .identity
        .grant_keys()
        .public_keys()
        .into_iter()
        .map(|(id, key, not_after)| {
            serde_json::json!({
                "id": id,
                "grant_public_key": hex::encode(key),
                "audit_public_key": audit.get(&id).map(hex::encode),
                "not_after": not_after.map(|t| t.millis()),
            })
        })
        .collect();
    axum::Json(serde_json::json!({ "keys": keys })).into_response()
}
