//! OIDC discovery.

use serde::Deserialize;

use crate::error::{Error, Result};

/// What a provider publishes at `/.well-known/openid-configuration`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ProviderMetadata {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
    #[serde(default)]
    pub device_authorization_endpoint: Option<String>,
    #[serde(default)]
    pub end_session_endpoint: Option<String>,
}

/// Fetch `issuer`'s metadata. The document must name the same issuer, so
/// a provider cannot vouch for another.
pub async fn discover(client: &reqwest::Client, issuer: &str) -> Result<ProviderMetadata> {
    let url = format!(
        "{}/.well-known/openid-configuration",
        issuer.trim_end_matches('/')
    );
    let meta: ProviderMetadata = client
        .get(&url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| Error::Oidc(format!("discovery at {url}: {e}")))?
        .json()
        .await
        .map_err(|e| Error::Oidc(format!("discovery at {url}: {e}")))?;
    if meta.issuer.trim_end_matches('/') != issuer.trim_end_matches('/') {
        return Err(Error::Oidc(format!(
            "{url} names issuer {:?}, not {issuer:?}",
            meta.issuer
        )));
    }
    Ok(meta)
}
