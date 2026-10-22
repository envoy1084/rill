//! The redacted-object list served to cache tiers.

use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use rill_runtime::state::AppState;

use super::{error_response, ObjectError};

/// The hashes of every redacted object, for cache tiers (service
/// principals) to drop and refuse, and for instance administrators. Kept
/// from everyone else: a small file's hash confirms a guess at it.
pub(super) async fn redactions(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Extension(ctx): axum::extract::Extension<rill_policy::AuthContext>,
) -> Response {
    let service =
        ctx.principal.kind == rill_types::PrincipalKind::Service && ctx.capability.is_none();
    if !service && crate::auth::require_instance_admin(&state, &ctx).is_err() {
        return error_response(ObjectError {
            status: StatusCode::FORBIDDEN,
            message: "the redaction list is for cache tiers and instance administrators".into(),
        });
    }
    let mut hashes: Vec<String> = state
        .identity
        .redacted_objects()
        .into_iter()
        .map(|(_, h)| h.to_hex())
        .collect();
    hashes.sort();
    hashes.dedup();
    axum::Json(serde_json::json!({ "hashes": hashes })).into_response()
}
