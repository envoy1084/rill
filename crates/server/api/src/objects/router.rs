//! The HTTP router: objects, sign-in, SCIM and health routes.

use std::sync::Arc;

use axum::middleware;
use axum::routing::{get, post};
use axum::Router;
use rill_runtime::state::AppState;

use super::{batch, handlers, health, keys, redactions};
use crate::auth::http::require_auth;

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
        .route("/redactions", get(redactions::redactions))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth))
        .layer(axum::extract::DefaultBodyLimit::max(MAX_PUT_BYTES));
    let health = Router::new()
        .route("/livez", get(health::livez))
        .route("/readyz", get(health::readyz))
        .route("/metrics", get(health::metrics))
        .route("/public-keys", get(keys::public_keys));
    let router = if serve_objects {
        health
            .merge(objects)
            .merge(crate::signin::router())
            .merge(crate::scim::router(state.clone()))
    } else {
        health
    };
    router.with_state(state)
}

/// Largest object accepted by `PUT`.
pub const MAX_PUT_BYTES: usize = 256 * 1024 * 1024;
