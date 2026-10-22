//! The browser flow: redirect to the provider, take its callback, keep
//! the session in an HttpOnly cookie, and hand the page short access
//! tokens from it. SAML providers start here too and end at
//! `/auth/saml/acs`.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Json;
use rill_identity::oidc::{begin, finish as finish_flow, Claims, Registration};
use rill_identity::signin::{open_session, resolve_user, RefreshToken};
use rill_types::identity::{IdentityProvider, PendingLogin, SessionKind, UserRecord};
use rill_types::Timestamp;
use serde::Deserialize;
use serde_json::json;

use super::{client_secret, page, provider, public_url, redirect_uri, session_ttl_ms};
use crate::error::{Error, Result};
use crate::state::AppState;

/// The cookie holding the session's refresh token.
const COOKIE: &str = "rill_session";
/// The header `/auth/token` and `/auth/logout` need, which a cross-site
/// form cannot send.
const CSRF_HEADER: &str = "x-rill-request";
/// How long a started sign-in may take.
pub(super) const LOGIN_TTL_MS: i64 = 600_000;

#[derive(Deserialize)]
pub struct LoginQuery {
    #[serde(default = "default_org")]
    org: String,
    provider: String,
    return_to: Option<String>,
}

pub(super) fn default_org() -> String {
    rill_store::DEFAULT_ORG.to_string()
}

/// Only paths on this server: never another origin.
fn safe_return(to: Option<String>) -> Option<String> {
    to.filter(|p| p.starts_with('/') && !p.starts_with("//") && !p.contains('\\'))
}

pub async fn login(State(state): State<Arc<AppState>>, Query(q): Query<LoginQuery>) -> Response {
    match start(&state, q).await {
        Ok(url) => Redirect::to(&url).into_response(),
        Err(e) => page::failed(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

async fn start(state: &AppState, q: LoginQuery) -> Result<String> {
    let provider = provider(state, &q.org, &q.provider).await?;
    if provider.saml.is_some() {
        #[cfg(feature = "saml")]
        return super::saml::start(state, &provider, safe_return(q.return_to)).await;
        #[cfg(not(feature = "saml"))]
        return Err(Error::invalid("this server was built without SAML support"));
    }
    let redirect = redirect_uri(state);
    let secret = client_secret(state, &provider).await?;
    let registration = Registration {
        issuer: &provider.issuer,
        client_id: &provider.client_id,
        client_secret: secret.as_deref(),
        redirect_uri: &redirect,
    };
    let request = begin(&registration, &provider.scopes).await?;
    let login = PendingLogin {
        state: request.state.clone(),
        org: provider.org.clone(),
        provider: provider.name.clone(),
        nonce: request.nonce.clone(),
        pkce_verifier: request.pkce_verifier.clone(),
        return_to: safe_return(q.return_to),
        expires_at: Timestamp::from_millis(Timestamp::now().millis() + LOGIN_TTL_MS),
    };
    state.identity.store().put_pending_login(&login).await?;
    Ok(request.url)
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

pub async fn callback(
    State(state): State<Arc<AppState>>,
    Query(q): Query<CallbackQuery>,
) -> Response {
    if let Some(error) = q.error {
        return page::failed(
            StatusCode::UNAUTHORIZED,
            &format!("the provider said: {error}"),
        );
    }
    let (Some(code), Some(login_state)) = (q.code, q.state) else {
        return page::failed(
            StatusCode::BAD_REQUEST,
            "the callback lacks a code or state",
        );
    };
    signed_in(finish(&state, &code, &login_state).await)
}

/// The page a finished sign-in lands on, setting the session cookie.
pub(super) fn signed_in(result: Result<(HeaderValue, UserRecord, Option<String>)>) -> Response {
    match result {
        Ok((cookie, user, return_to)) => {
            let mut response = match return_to {
                Some(to) => Redirect::to(&to).into_response(),
                None => page::signed_in(&user.principal.name, &user.org),
            };
            response.headers_mut().insert(header::SET_COOKIE, cookie);
            response
        }
        Err(e) => {
            tracing::info!(error = %e, "sign-in failed");
            page::failed(StatusCode::UNAUTHORIZED, &e.to_string())
        }
    }
}

async fn finish(
    state: &AppState,
    code: &str,
    login_state: &str,
) -> Result<(HeaderValue, UserRecord, Option<String>)> {
    let now = Timestamp::now();
    let login = state
        .identity
        .store()
        .take_pending_login(login_state, now)
        .await?
        .ok_or_else(|| Error::Unauthenticated("this sign-in expired or was already used".into()))?;
    let provider = provider(state, &login.org, &login.provider).await?;
    let secret = client_secret(state, &provider).await?;
    let redirect = redirect_uri(state);
    let registration = Registration {
        issuer: &provider.issuer,
        client_id: &provider.client_id,
        client_secret: secret.as_deref(),
        redirect_uri: &redirect,
    };
    let claims = finish_flow(&registration, code, &login.pkce_verifier, &login.nonce).await?;
    let (cookie, user) = sign_in(state, &provider, &claims, now).await?;
    Ok((cookie, user, login.return_to))
}

/// The user verified `claims` name, and a browser session cookie for them.
pub(super) async fn sign_in(
    state: &AppState,
    provider: &IdentityProvider,
    claims: &Claims,
    now: Timestamp,
) -> Result<(HeaderValue, UserRecord)> {
    let user = resolve_user(state.identity.store(), provider, claims, now).await?;
    let issuer = state.identity.issuer()?;
    let session = open_session(
        state.identity.store(),
        &issuer,
        &user,
        SessionKind::Browser,
        session_ttl_ms(state),
        now,
    )
    .await?;
    tracing::info!(user = %user.principal, org = %user.org, session = %session.session.id, "signed in");
    let cookie = session_cookie(
        state,
        &session.refresh_token.to_string(),
        session_ttl_ms(state) / 1000,
    )?;
    Ok((cookie, user))
}

fn session_cookie(state: &AppState, value: &str, max_age_secs: i64) -> Result<HeaderValue> {
    let secure = if public_url(state).starts_with("https://") {
        "; Secure"
    } else {
        ""
    };
    HeaderValue::from_str(&format!(
        "{COOKIE}={value}; HttpOnly; SameSite=Lax; Path=/auth; Max-Age={max_age_secs}{secure}"
    ))
    .map_err(|e| Error::internal(format!("cookie: {e}")))
}

fn cookie_value(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == COOKIE)
        .map(|(_, value)| value.to_string())
}

fn csrf_ok(headers: &HeaderMap) -> bool {
    headers.contains_key(CSRF_HEADER)
}

fn unauthorized(reason: &str) -> Response {
    (StatusCode::UNAUTHORIZED, Json(json!({ "error": reason }))).into_response()
}

/// Trade the session cookie for an access token (rotating the cookie).
pub async fn token(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if !csrf_ok(&headers) {
        return unauthorized("missing x-rill-request header");
    }
    let Some(refresh) = cookie_value(&headers).and_then(|v| v.parse::<RefreshToken>().ok()) else {
        return unauthorized("not signed in");
    };
    let result = async {
        let tokens = super::flows::refresh_live(&state, &refresh).await?;
        let remaining = (tokens.session.expires_at.millis() - Timestamp::now().millis()) / 1000;
        let cookie = session_cookie(&state, &tokens.refresh_token.to_string(), remaining.max(0))?;
        Ok::<_, Error>((cookie, tokens))
    }
    .await;
    match result {
        Ok((cookie, tokens)) => {
            let mut response = Json(json!({
                "access_token": tokens.access_token,
                "expires_at": tokens.expires_at.millis(),
                "principal": tokens.session.principal.name,
                "org": tokens.session.org,
            }))
            .into_response();
            response.headers_mut().insert(header::SET_COOKIE, cookie);
            response
        }
        Err(e) => unauthorized(&e.to_string()),
    }
}

/// End the cookie's session.
pub async fn logout(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if !csrf_ok(&headers) {
        return unauthorized("missing x-rill-request header");
    }
    if let Some(refresh) = cookie_value(&headers).and_then(|v| v.parse::<RefreshToken>().ok()) {
        let store = state.identity.store();
        if let Ok(Some(session)) = store.session(&refresh.session).await {
            match store.revoke_session(&session.id, &session.principal).await {
                Ok(()) => {
                    state
                        .identity
                        .apply(&rill_types::identity::IdentityChange::SessionRevoked {
                            session: session.id,
                        });
                }
                Err(e) => tracing::warn!(error = %e, "ending a session failed"),
            }
        }
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    if let Ok(cookie) = session_cookie(&state, "", 0) {
        response.headers_mut().insert(header::SET_COOKIE, cookie);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_in_returns_only_to_this_server() {
        assert_eq!(
            safe_return(Some("/repos".into())).as_deref(),
            Some("/repos")
        );
        assert_eq!(safe_return(Some("//evil.example".into())), None);
        assert_eq!(safe_return(Some("https://evil.example".into())), None);
        assert_eq!(safe_return(Some("/\\evil.example".into())), None);
    }
}
