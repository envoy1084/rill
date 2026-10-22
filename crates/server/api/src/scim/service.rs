//! The per-organization SCIM service.

use rill_identity::scim::Scim;
use rill_runtime::state::AppState;

/// The SCIM service of `org`.
pub fn scim_for(state: &AppState, org: &str) -> Scim {
    let base = match &state.config.identity.public_url {
        Some(url) => url.trim_end_matches('/').to_string(),
        None => format!("http://{}", state.config.listen_http),
    };
    Scim::new(
        state.identity.store().clone(),
        org,
        format!("{base}/scim/v2/{org}"),
    )
}
