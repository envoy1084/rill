//! The authorization-code flow with PKCE (RFC 7636).

use base64::Engine as _;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::discovery::ProviderMetadata;
use crate::error::{Error, Result};

fn b64url() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
}

fn random_token() -> String {
    b64url().encode(rand::random::<[u8; 32]>())
}

/// The secrets of one sign-in: `state` ties the callback to it, `nonce`
/// ties the ID token to it, and the PKCE verifier proves the code was
/// requested by whoever redeems it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRequest {
    pub state: String,
    pub nonce: String,
    pub pkce_verifier: String,
}

impl AuthorizationRequest {
    pub fn new() -> Self {
        AuthorizationRequest {
            state: random_token(),
            nonce: random_token(),
            pkce_verifier: random_token(),
        }
    }

    /// The S256 challenge for the verifier.
    pub fn pkce_challenge(&self) -> String {
        b64url().encode(Sha256::digest(self.pkce_verifier.as_bytes()))
    }
}

impl Default for AuthorizationRequest {
    fn default() -> Self {
        Self::new()
    }
}

/// Where to send the browser to sign in.
pub fn authorize_url(
    meta: &ProviderMetadata,
    client_id: &str,
    redirect_uri: &str,
    scopes: &[String],
    request: &AuthorizationRequest,
) -> Result<String> {
    let mut url = url::Url::parse(&meta.authorization_endpoint)
        .map_err(|e| Error::Oidc(format!("authorization endpoint: {e}")))?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", &scopes.join(" "))
        .append_pair("state", &request.state)
        .append_pair("nonce", &request.nonce)
        .append_pair("code_challenge", &request.pkce_challenge())
        .append_pair("code_challenge_method", "S256");
    Ok(url.into())
}

/// The token endpoint's answer to a code exchange.
#[derive(Debug, Clone, Deserialize)]
pub struct CodeExchange {
    pub id_token: String,
    #[serde(default)]
    pub access_token: Option<String>,
}

/// Redeem `code` for tokens.
pub async fn exchange_code(
    client: &reqwest::Client,
    meta: &ProviderMetadata,
    client_id: &str,
    client_secret: Option<&str>,
    redirect_uri: &str,
    code: &str,
    pkce_verifier: &str,
) -> Result<CodeExchange> {
    let mut form = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
        ("code_verifier", pkce_verifier),
    ];
    if let Some(secret) = client_secret {
        form.push(("client_secret", secret));
    }
    let response = client
        .post(&meta.token_endpoint)
        .form(&form)
        .send()
        .await
        .map_err(|e| Error::Oidc(format!("token endpoint: {e}")))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| Error::Oidc(format!("token endpoint: {e}")))?;
    if !status.is_success() {
        return Err(Error::Oidc(format!(
            "token endpoint returned {status}: {body}"
        )));
    }
    serde_json::from_str(&body).map_err(|e| Error::Oidc(format!("token endpoint: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_is_rfc7636_s256() {
        // The example from RFC 7636, appendix B.
        let request = AuthorizationRequest {
            state: String::new(),
            nonce: String::new(),
            pkce_verifier: "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".into(),
        };
        assert_eq!(
            request.pkce_challenge(),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        let a = AuthorizationRequest::new();
        let b = AuthorizationRequest::new();
        assert_ne!(a.state, b.state);
        assert_ne!(a.pkce_verifier, a.nonce);
    }
}
