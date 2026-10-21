//! `POST /oauth/token` (RFC 6749 and RFC 8693): refresh a session, or
//! exchange an identity provider's ID token for a Rill session. The CLI's
//! device flow ends here.

use std::sync::Arc;

use axum::extract::{Form, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use rill_identity::signin::{RefreshToken, SessionTokens};
use rill_types::Timestamp;
use serde::Deserialize;
use serde_json::json;

use crate::error::{Error, Result};
use crate::state::AppState;

pub const GRANT_REFRESH: &str = "refresh_token";
pub const GRANT_EXCHANGE: &str = "urn:ietf:params:oauth:grant-type:token-exchange";
pub const TOKEN_TYPE_ID: &str = "urn:ietf:params:oauth:token-type:id_token";
pub const TOKEN_TYPE_ACCESS: &str = "urn:ietf:params:oauth:token-type:access_token";

#[derive(Deserialize)]
pub struct TokenForm {
    grant_type: String,
    refresh_token: Option<String>,
    subject_token: Option<String>,
    subject_token_type: Option<String>,
}

/// An RFC 6749 error response.
fn oauth_error(status: StatusCode, error: &str, description: &str) -> Response {
    (
        status,
        Json(json!({ "error": error, "error_description": description })),
    )
        .into_response()
}

fn granted(tokens: SessionTokens) -> Response {
    let now = Timestamp::now().millis();
    Json(json!({
        "access_token": tokens.access_token,
        "issued_token_type": TOKEN_TYPE_ACCESS,
        "token_type": "Bearer",
        "expires_in": (tokens.expires_at.millis() - now).max(0) / 1000,
        "refresh_token": tokens.refresh_token.to_string(),
        "session": tokens.session.id.to_string(),
    }))
    .into_response()
}

pub async fn token(State(state): State<Arc<AppState>>, Form(form): Form<TokenForm>) -> Response {
    let result = match form.grant_type.as_str() {
        GRANT_REFRESH => refresh(&state, form.refresh_token.as_deref()).await,
        GRANT_EXCHANGE => match form.subject_token_type.as_deref() {
            Some(TOKEN_TYPE_ID) => match form.subject_token.as_deref() {
                Some(token) => super::flows::exchange_id_token(&state, token).await,
                None => Err(Error::invalid("subject_token is required")),
            },
            other => {
                return oauth_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    &format!("unsupported subject_token_type {other:?}"),
                )
            }
        },
        other => {
            return oauth_error(
                StatusCode::BAD_REQUEST,
                "unsupported_grant_type",
                &format!("grant_type {other:?}"),
            )
        }
    };
    match result {
        Ok(tokens) => granted(tokens),
        Err(e @ Error::Unavailable(_)) => oauth_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "temporarily_unavailable",
            &e.to_string(),
        ),
        Err(e) => {
            tracing::info!(error = %e, "token request refused");
            oauth_error(StatusCode::BAD_REQUEST, "invalid_grant", &e.to_string())
        }
    }
}

async fn refresh(state: &AppState, token: Option<&str>) -> Result<SessionTokens> {
    let token: RefreshToken = token
        .ok_or_else(|| Error::invalid("refresh_token is required"))?
        .parse()?;
    super::flows::refresh_live(state, &token).await
}
