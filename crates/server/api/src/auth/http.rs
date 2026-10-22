//! Axum middleware mirroring [`super::layer`] for the object protocol.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::header::{AUTHORIZATION, RETRY_AFTER};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use rill_policy::AuthContext;

use super::authenticator::authenticate;
use rill_runtime::error::Error;
use rill_runtime::state::AppState;

/// Resolve the caller and attach it to the request, or reply 401 (or
/// 429 past its rate limit).
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
    let mut ctx = match authenticate(&state, header.as_deref()).await {
        Ok(ctx) => ctx,
        Err(e) => return (StatusCode::UNAUTHORIZED, e.to_string()).into_response(),
    };
    if let Err(e) = state.admit(&ctx) {
        return too_many_requests(e);
    }
    ctx.relations_seen = super::context::relations_seen(seen.as_deref());
    request.extensions_mut().insert::<AuthContext>(ctx);
    next.run(request).await
}

fn too_many_requests(e: Error) -> Response {
    let retry_after = match &e {
        Error::RateLimited {
            retry_after_secs, ..
        } => *retry_after_secs,
        _ => 1,
    };
    (
        StatusCode::TOO_MANY_REQUESTS,
        [(RETRY_AFTER, retry_after.to_string())],
        e.to_string(),
    )
        .into_response()
}
