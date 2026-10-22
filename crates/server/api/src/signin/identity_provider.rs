//! Looking up an organization's identity provider and its client secret.

use std::sync::Arc;

use rill_identity::keys::KeyWrapper;
use rill_runtime::error::{Error, Result};
use rill_runtime::state::AppState;
use rill_types::identity::IdentityProvider;

pub(super) async fn provider(state: &AppState, org: &str, name: &str) -> Result<IdentityProvider> {
    state
        .identity
        .store()
        .provider(org, name)
        .await?
        .ok_or_else(|| Error::NotFound(format!("no identity provider {org}/{name}")))
}

/// The provider's client secret, unwrapped by the key service.
pub(super) async fn client_secret(
    state: &AppState,
    provider: &IdentityProvider,
) -> Result<Option<String>> {
    let Some(wrapped) = &provider.client_secret else {
        return Ok(None);
    };
    let service: &Arc<dyn KeyWrapper> = state.identity.keys().key_service();
    let bytes = service.unwrap(wrapped).await?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| Error::internal("stored client secret is not text"))
}
