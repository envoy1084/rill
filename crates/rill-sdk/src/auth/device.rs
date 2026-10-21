//! The OAuth device authorization grant (RFC 8628) against an identity
//! provider: show a code, wait for the person to approve it in a browser,
//! receive an ID token.

use std::time::Duration;

use serde::Deserialize;

use crate::error::{Error, Result};

const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// What the person must do to approve the sign-in.
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceAuthorization {
    /// What the client polls with; secret.
    pub device_code: String,
    /// What the person types at `verification_uri`.
    pub user_code: String,
    /// Where the person approves.
    pub verification_uri: String,
    /// `verification_uri` with the code filled in.
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    /// Seconds until the code expires.
    pub expires_in: u64,
    /// Seconds between polls.
    #[serde(default)]
    pub interval: Option<u64>,
}

/// A device flow with one provider client.
#[derive(Debug, Clone)]
pub struct DeviceFlow {
    /// The provider's public client for the CLI.
    pub client_id: String,
    /// Scopes to ask for.
    pub scopes: Vec<String>,
    /// Where the code is requested.
    pub device_authorization_endpoint: String,
    /// Where the client polls.
    pub token_endpoint: String,
}

#[derive(Deserialize)]
struct TokenAnswer {
    id_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

impl DeviceFlow {
    /// Ask the provider for a code to show.
    pub async fn start(&self, http: &reqwest::Client) -> Result<DeviceAuthorization> {
        let scope = self.scopes.join(" ");
        http.post(&self.device_authorization_endpoint)
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("scope", scope.as_str()),
            ])
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| Error::config(format!("device authorization: {e}")))?
            .json()
            .await
            .map_err(|e| Error::config(format!("device authorization: {e}")))
    }

    /// Poll until the person approves (the ID token), denies, or the code
    /// expires.
    pub async fn wait(
        &self,
        http: &reqwest::Client,
        authorization: &DeviceAuthorization,
    ) -> Result<String> {
        let mut interval = Duration::from_secs(authorization.interval.unwrap_or(5).max(1));
        let deadline = tokio::time::Instant::now() + Duration::from_secs(authorization.expires_in);
        loop {
            tokio::time::sleep(interval).await;
            if tokio::time::Instant::now() >= deadline {
                return Err(Error::config("the sign-in code expired"));
            }
            let answer: TokenAnswer = http
                .post(&self.token_endpoint)
                .form(&[
                    ("grant_type", DEVICE_GRANT),
                    ("device_code", authorization.device_code.as_str()),
                    ("client_id", self.client_id.as_str()),
                ])
                .send()
                .await
                .map_err(|e| Error::config(format!("token endpoint: {e}")))?
                .json()
                .await
                .map_err(|e| Error::config(format!("token endpoint: {e}")))?;
            if let Some(id_token) = answer.id_token {
                return Ok(id_token);
            }
            match answer.error.as_deref() {
                Some("authorization_pending") => {}
                Some("slow_down") => interval += Duration::from_secs(5),
                Some(other) => {
                    return Err(Error::config(format!(
                        "sign-in refused: {other} {}",
                        answer.error_description.unwrap_or_default()
                    )))
                }
                None => return Err(Error::config("the provider returned no ID token")),
            }
        }
    }
}
