//! SAML sign-in: the AuthnRequest redirect, the assertion consumer that
//! takes the IdP's signed response, and Rill's metadata for the IdP.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Form;
use rill_identity::saml::SamlProvider;
use rill_types::identity::{IdentityProvider, PendingLogin, UserRecord};
use rill_types::Timestamp;
use serde::Deserialize;

use super::browser::{default_org, sign_in, signed_in, LOGIN_TTL_MS};
use super::{page, provider, public_url};
use crate::error::{Error, Result};
use crate::state::AppState;

/// Where the IdP posts responses; the RelayState says which sign-in.
fn acs_url(state: &AppState) -> String {
    format!("{}/auth/saml/acs", public_url(state))
}

fn service_provider(state: &AppState, provider: &IdentityProvider) -> Result<SamlProvider> {
    let settings = provider.saml.as_ref().ok_or_else(|| {
        Error::invalid(format!(
            "{}/{} is not a SAML provider",
            provider.org, provider.name
        ))
    })?;
    Ok(SamlProvider::new(settings, &acs_url(state))?)
}

/// Record the sign-in and return the IdP URL to redirect to.
pub(super) async fn start(
    state: &AppState,
    provider: &IdentityProvider,
    return_to: Option<String>,
) -> Result<String> {
    let sp = service_provider(state, provider)?;
    let relay_state = hex::encode(rand::random::<[u8; 16]>());
    let request = sp.request(&relay_state)?;
    let login = PendingLogin {
        state: relay_state,
        org: provider.org.clone(),
        provider: provider.name.clone(),
        nonce: request.id,
        pkce_verifier: String::new(),
        return_to,
        expires_at: Timestamp::from_millis(Timestamp::now().millis() + LOGIN_TTL_MS),
    };
    state.identity.store().put_pending_login(&login).await?;
    Ok(request.redirect_url)
}

#[derive(Deserialize)]
pub struct AcsForm {
    #[serde(rename = "SAMLResponse")]
    saml_response: String,
    #[serde(rename = "RelayState")]
    relay_state: Option<String>,
}

pub async fn acs(State(state): State<Arc<AppState>>, Form(form): Form<AcsForm>) -> Response {
    let Some(relay_state) = form.relay_state else {
        return page::failed(
            StatusCode::BAD_REQUEST,
            "the response has no RelayState; sign-ins start at Rill",
        );
    };
    signed_in(finish(&state, form.saml_response, &relay_state).await)
}

async fn finish(
    state: &AppState,
    response: String,
    relay_state: &str,
) -> Result<(HeaderValue, UserRecord, Option<String>)> {
    let now = Timestamp::now();
    let login = state
        .identity
        .store()
        .take_pending_login(relay_state, now)
        .await?
        .ok_or_else(|| Error::Unauthenticated("this sign-in expired or was already used".into()))?;
    let provider = provider(state, &login.org, &login.provider).await?;
    let sp = service_provider(state, &provider)?;
    let request_id = login.nonce;
    let claims = tokio::task::spawn_blocking(move || sp.verify(&response, &request_id))
        .await
        .map_err(|e| Error::internal(format!("SAML verification: {e}")))??;
    let (cookie, user) = sign_in(state, &provider, &claims, now).await?;
    Ok((cookie, user, login.return_to))
}

#[derive(Deserialize)]
pub struct MetadataQuery {
    #[serde(default = "default_org")]
    org: String,
    provider: String,
}

/// Rill's service provider metadata for one provider.
pub async fn metadata(
    State(state): State<Arc<AppState>>,
    Query(q): Query<MetadataQuery>,
) -> Response {
    let result = async {
        let provider = provider(&state, &q.org, &q.provider).await?;
        Ok::<_, Error>(service_provider(&state, &provider)?.sp_metadata())
    }
    .await;
    match result {
        Ok(xml) => (
            [(header::CONTENT_TYPE, "application/samlmetadata+xml")],
            xml,
        )
            .into_response(),
        Err(e) => (StatusCode::NOT_FOUND, e.to_string()).into_response(),
    }
}
