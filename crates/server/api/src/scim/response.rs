//! SCIM JSON responses and errors.

use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use rill_identity::scim::ScimError;
use serde_json::Value;

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
