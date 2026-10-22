//! Liveness, readiness and metrics endpoints, served without auth.

use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use rill_runtime::state::AppState;

/// The process is up and serving HTTP.
pub(super) async fn livez() -> &'static str {
    "ok"
}

/// Every metric, in the Prometheus text format.
pub(super) async fn metrics(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> Response {
    (
        [(
            http::header::CONTENT_TYPE,
            "application/openmetrics-text; version=1.0.0; charset=utf-8",
        )],
        state.metrics.encode(),
    )
        .into_response()
}

pub(super) async fn readyz(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> Response {
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
