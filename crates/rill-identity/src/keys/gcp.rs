//! Google Cloud KMS as a key service, over its REST API.

use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine as _;
use rill_types::identity::WrappedSecret;
use serde_json::json;

use super::wrapper::KeyWrapper;
use crate::error::{Error, Result};

const BACKEND: &str = "gcp-kms";
const SCOPE: &str = "https://www.googleapis.com/auth/cloudkms";
const ENDPOINT: &str = "https://cloudkms.googleapis.com";

/// Wraps secrets with a symmetric Cloud KMS key:
/// `POST /v1/{key_name}:encrypt`.
pub struct GcpKms {
    client: reqwest::Client,
    auth: Arc<dyn gcp_auth::TokenProvider>,
    endpoint: String,
    key_name: String,
}

impl GcpKms {
    /// A client using application default credentials, against `endpoint`
    /// when given.
    pub async fn connect(key_name: &str, endpoint: Option<&str>) -> Result<Self> {
        let auth = gcp_auth::provider()
            .await
            .map_err(|e| Error::key_service(BACKEND, e))?;
        Ok(Self::with_auth(key_name, endpoint, auth))
    }

    pub fn with_auth(
        key_name: &str,
        endpoint: Option<&str>,
        auth: Arc<dyn gcp_auth::TokenProvider>,
    ) -> Self {
        GcpKms {
            client: reqwest::Client::new(),
            auth,
            endpoint: endpoint
                .unwrap_or(ENDPOINT)
                .trim_end_matches('/')
                .to_string(),
            key_name: key_name.to_string(),
        }
    }

    async fn call(&self, op: &str, body: serde_json::Value) -> Result<serde_json::Value> {
        let token = self
            .auth
            .token(&[SCOPE])
            .await
            .map_err(|e| Error::key_service(BACKEND, e))?;
        let url = format!("{}/v1/{}:{op}", self.endpoint, self.key_name);
        let response = self
            .client
            .post(&url)
            .bearer_auth(token.as_str())
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::key_service(BACKEND, e))?;
        let status = response.status();
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| Error::key_service(BACKEND, e))?;
        if !status.is_success() {
            return Err(Error::key_service(
                BACKEND,
                format!("{op} returned {status}: {}", body["error"]["message"]),
            ));
        }
        Ok(body)
    }
}

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

fn aad() -> String {
    b64().encode("rill-token-key")
}

#[async_trait]
impl KeyWrapper for GcpKms {
    fn backend(&self) -> &'static str {
        BACKEND
    }

    fn key_ref(&self) -> &str {
        &self.key_name
    }

    async fn wrap(&self, plaintext: &[u8]) -> Result<WrappedSecret> {
        let body = self
            .call(
                "encrypt",
                json!({ "plaintext": b64().encode(plaintext), "additionalAuthenticatedData": aad() }),
            )
            .await?;
        let ciphertext = body["ciphertext"]
            .as_str()
            .ok_or_else(|| Error::key_service(BACKEND, "encrypt returned no ciphertext"))?;
        Ok(WrappedSecret {
            backend: BACKEND.into(),
            key_ref: self.key_name.clone(),
            ciphertext: b64()
                .decode(ciphertext)
                .map_err(|e| Error::key_service(BACKEND, e))?,
        })
    }

    async fn unwrap(&self, wrapped: &WrappedSecret) -> Result<Vec<u8>> {
        if !self.can_unwrap(wrapped) {
            return Err(Error::key_service(
                BACKEND,
                format!(
                    "secret was wrapped by {}/{}",
                    wrapped.backend, wrapped.key_ref
                ),
            ));
        }
        let body = self
            .call(
                "decrypt",
                json!({ "ciphertext": b64().encode(&wrapped.ciphertext), "additionalAuthenticatedData": aad() }),
            )
            .await?;
        let plaintext = body["plaintext"]
            .as_str()
            .ok_or_else(|| Error::key_service(BACKEND, "decrypt returned no plaintext"))?;
        b64()
            .decode(plaintext)
            .map_err(|e| Error::key_service(BACKEND, e))
    }
}
