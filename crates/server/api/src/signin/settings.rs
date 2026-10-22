//! Sign-in settings derived from the server configuration.

use rill_runtime::state::AppState;

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

pub(super) fn redirect_uri(state: &AppState) -> String {
    format!("{}/auth/callback", public_url(state))
}

/// How long sign-in sessions last.
pub(super) fn session_ttl_ms(state: &AppState) -> i64 {
    (state.config.identity.session_hours as i64).saturating_mul(3_600_000)
}
