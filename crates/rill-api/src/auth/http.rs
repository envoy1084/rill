//! Axum middleware mirroring [`super::layer`] for the object protocol.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::header::AUTHORIZATION;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use rill_policy::AuthContext;

use super::authenticator::authenticate;
use crate::state::AppState;

/// Resolve the caller and attach it to the request, or reply 401.
pub async fn require_auth(
    State(state): State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Response {
    let header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let seen = request
        .headers()
        .get(super::context::RELATIONS_SEEN_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    match authenticate(&state, header.as_deref()).await {
        Ok(mut ctx) => {
            ctx.relations_seen = super::context::relations_seen(seen.as_deref());
            request.extensions_mut().insert::<AuthContext>(ctx);
            next.run(request).await
        }
        Err(e) => (StatusCode::UNAUTHORIZED, e.to_string()).into_response(),
    }
}
