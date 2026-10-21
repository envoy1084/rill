//! The object protocol: plain HTTP access to content-addressed objects.
//!
//! This is the surface that caches and CDNs sit in front of. Every object
//! URL is immutable (`/o/{repo}/{kind}/{hash}` names exactly one byte
//! string forever), responses carry `ETag` and a one-year immutable
//! `Cache-Control`, and reads need only the bearer token and a read
//! relation on the repository, so an edge cache can serve a hot
//! repository's trees and blobs without touching the gRPC API. Writes go
//! through `PUT` with the hash verified server-side.
//!
//! Routes:
//!
//! * `GET|HEAD /o/{repo}/{kind}/{hash}`: one object;
//! * `PUT /o/{repo}/{kind}/{hash}`: store one object (400 on hash mismatch);
//! * `POST /o/{repo}/batch`: many objects, length-prefixed (see [`batch`]);
//! * `GET /healthz`, `GET /readyz`: liveness and readiness, no auth.

pub mod batch;
pub mod handlers;

use std::sync::Arc;

use axum::http::StatusCode;
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use tonic::Code;

use crate::auth::http::require_auth;
use crate::error::Error;
use crate::state::AppState;

/// An HTTP-level error.
#[derive(Debug)]
pub struct ObjectError {
    status: StatusCode,
    message: String,
}

impl ObjectError {
    /// A 400.
    pub fn bad_request(message: impl Into<String>) -> Self {
        ObjectError {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    /// A 404.
    pub fn not_found(message: impl Into<String>) -> Self {
        ObjectError {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }
}

impl From<Error> for ObjectError {
    fn from(err: Error) -> Self {
        let status = match err.code() {
            Code::NotFound => StatusCode::NOT_FOUND,
            Code::PermissionDenied => StatusCode::FORBIDDEN,
            Code::Unauthenticated => StatusCode::UNAUTHORIZED,
            Code::InvalidArgument | Code::FailedPrecondition => StatusCode::BAD_REQUEST,
            Code::AlreadyExists | Code::Aborted => StatusCode::CONFLICT,
            Code::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        ObjectError {
            status,
            message: err.to_string(),
        }
    }
}

impl From<rill_store::Error> for ObjectError {
    fn from(err: rill_store::Error) -> Self {
        Error::from(err).into()
    }
}

/// Render an error as a plain-text response.
pub fn error_response(err: ObjectError) -> Response {
    if err.status.is_server_error() {
        tracing::error!(error = %err.message, "object protocol error");
    }
    (err.status, err.message).into_response()
}

/// Build the HTTP router.
pub fn router(state: Arc<AppState>, serve_objects: bool) -> Router {
    let objects = Router::new()
        .route(
            "/o/{repo}/{kind}/{hash}",
            get(handlers::get_object)
                .head(handlers::head_object)
                .put(handlers::put_object),
        )
        .route("/o/{repo}/batch", post(batch::batch))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth))
        .layer(axum::extract::DefaultBodyLimit::max(MAX_PUT_BYTES));
    let health = Router::new()
        .route("/livez", get(livez))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics));
    let router = if serve_objects {
        health.merge(objects)
    } else {
        health
    };
    router.with_state(state)
}

/// Largest object accepted by `PUT`.
pub const MAX_PUT_BYTES: usize = 256 * 1024 * 1024;

/// The process is up and serving HTTP.
async fn livez() -> &'static str {
    "ok"
}

/// Every metric, in the Prometheus text format.
async fn metrics(axum::extract::State(state): axum::extract::State<Arc<AppState>>) -> Response {
    (
        [(
            http::header::CONTENT_TYPE,
            "application/openmetrics-text; version=1.0.0; charset=utf-8",
        )],
        state.metrics.encode(),
    )
        .into_response()
}

async fn readyz(axum::extract::State(state): axum::extract::State<Arc<AppState>>) -> Response {
    if state.lifecycle.is_draining() {
        return (StatusCode::SERVICE_UNAVAILABLE, "draining").into_response();
    }
    let components = crate::services::admin::health_components(&state).await;
    if components.iter().all(|c| c.ok) {
        (StatusCode::OK, "ready").into_response()
    } else {
        let detail: Vec<String> = components
            .iter()
            .filter(|c| !c.ok)
            .map(|c| format!("{}: {}", c.name, c.detail.clone().unwrap_or_default()))
            .collect();
        (StatusCode::SERVICE_UNAVAILABLE, detail.join("\n")).into_response()
    }
}
