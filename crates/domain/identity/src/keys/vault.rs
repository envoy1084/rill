//! HashiCorp Vault's Transit engine as a key service.

use async_trait::async_trait;
use base64::Engine as _;
use rill_types::identity::WrappedSecret;
use serde_json::json;

use super::wrapper::KeyWrapper;
use crate::error::{Error, Result};

const BACKEND: &str = "vault";

/// Wraps secrets with a Transit key: `POST /v1/<mount>/encrypt/<key>`.
pub struct VaultTransit {
    client: reqwest::Client,
    address: String,
    token: String,
    mount: String,
    key: String,
    key_ref: String,
}

impl VaultTransit {
    pub fn new(address: &str, token: &str, mount: &str, key: &str) -> Self {
        let address = address.trim_end_matches('/').to_string();
        VaultTransit {
            client: reqwest::Client::new(),
            key_ref: format!("{mount}/{key}"),
            address,
            token: token.to_string(),
            mount: mount.to_string(),
            key: key.to_string(),
        }
    }

    async fn call(&self, op: &str, body: serde_json::Value) -> Result<serde_json::Value> {
        let url = format!("{}/v1/{}/{op}/{}", self.address, self.mount, self.key);
        let response = self
            .client
            .post(&url)
            .header("X-Vault-Token", &self.token)
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
                format!("{op} returned {status}: {}", body["errors"]),
            ));
        }
        Ok(body)
    }
}

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

#[async_trait]
impl KeyWrapper for VaultTransit {
    fn backend(&self) -> &'static str {
        BACKEND
    }

    fn key_ref(&self) -> &str {
        &self.key_ref
    }

    async fn wrap(&self, plaintext: &[u8]) -> Result<WrappedSecret> {
        let body = self
            .call("encrypt", json!({ "plaintext": b64().encode(plaintext) }))
            .await?;
        let ciphertext = body["data"]["ciphertext"]
            .as_str()
            .ok_or_else(|| Error::key_service(BACKEND, "encrypt returned no ciphertext"))?;
        Ok(WrappedSecret {
            backend: BACKEND.into(),
            key_ref: self.key_ref.clone(),
            ciphertext: ciphertext.as_bytes().to_vec(),
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
        let ciphertext = std::str::from_utf8(&wrapped.ciphertext)
            .map_err(|_| Error::key_service(BACKEND, "stored ciphertext is not text"))?;
        let body = self
            .call("decrypt", json!({ "ciphertext": ciphertext }))
            .await?;
        let plaintext = body["data"]["plaintext"]
            .as_str()
            .ok_or_else(|| Error::key_service(BACKEND, "decrypt returned no plaintext"))?;
        b64()
            .decode(plaintext)
            .map_err(|e| Error::key_service(BACKEND, e))
    }
}
