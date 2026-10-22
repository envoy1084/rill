//! Signing in over HTTP: the browser flow through an organization's
//! identity provider, the OAuth token endpoint that refreshes sessions and
//! exchanges ID tokens (the CLI's device flow ends here), and the provider
//! list clients start from. With the `saml` feature, SAML 2.0 providers
//! sign browsers in too. These routes are open; each checks what it is
//! given.

mod browser;
pub mod flows;
mod oauth;
mod page;
mod providers;
#[cfg(feature = "saml")]
mod saml;

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use rill_identity::keys::KeyWrapper;
use rill_types::identity::IdentityProvider;

use rill_runtime::error::{Error, Result};
use rill_runtime::state::AppState;

/// The sign-in routes.
pub fn router() -> Router<Arc<AppState>> {
    #[cfg(feature = "saml")]
    let router = Router::new()
        .route("/auth/saml/acs", post(saml::acs))
        .route("/auth/saml/metadata", get(saml::metadata));
    #[cfg(not(feature = "saml"))]
    let router = Router::new();
    router
        .route("/auth/login", get(browser::login))
        .route("/auth/callback", get(browser::callback))
        .route("/auth/token", post(browser::token))
        .route("/auth/logout", post(browser::logout))
        .route("/auth/providers", get(providers::list))
        .route("/oauth/token", post(oauth::token))
}

/// Where browsers reach this server.
pub(crate) fn public_url(state: &AppState) -> String {
    match &state.config.identity.public_url {
        Some(url) => url.trim_end_matches('/').to_string(),
        None => {
            let scheme = if state.config.tls.is_some() {
                "https"
            } else {
                "http"
            };
            format!("{scheme}://{}", state.config.listen_http)
        }
    }
}

fn redirect_uri(state: &AppState) -> String {
    format!("{}/auth/callback", public_url(state))
}

/// How long sign-in sessions last.
fn session_ttl_ms(state: &AppState) -> i64 {
    (state.config.identity.session_hours as i64).saturating_mul(3_600_000)
}

async fn provider(state: &AppState, org: &str, name: &str) -> Result<IdentityProvider> {
    state
        .identity
        .store()
        .provider(org, name)
        .await?
        .ok_or_else(|| Error::NotFound(format!("no identity provider {org}/{name}")))
}

/// The provider's client secret, unwrapped by the key service.
async fn client_secret(state: &AppState, provider: &IdentityProvider) -> Result<Option<String>> {
    let Some(wrapped) = &provider.client_secret else {
        return Ok(None);
    };
    let service: &Arc<dyn KeyWrapper> = state.identity.keys().key_service();
    let bytes = service.unwrap(wrapped).await?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| Error::internal("stored client secret is not text"))
}
