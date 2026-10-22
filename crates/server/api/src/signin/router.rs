//! The sign-in routes.

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use rill_runtime::state::AppState;

#[cfg(feature = "saml")]
use super::saml;
use super::{browser, oauth, providers};

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
