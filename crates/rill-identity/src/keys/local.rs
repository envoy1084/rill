//! A key service backed by a local AES-256-GCM master key, for
//! development and single-host installs.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use async_trait::async_trait;
use rill_types::identity::WrappedSecret;

use super::wrapper::KeyWrapper;
use crate::error::{Error, Result};

const BACKEND: &str = "local";
const NONCE_LEN: usize = 12;

/// Wraps secrets with a 32-byte master key.
pub struct LocalKeyService {
    cipher: Aes256Gcm,
    /// Identifies the master key without revealing it.
    fingerprint: String,
}

impl LocalKeyService {
    pub fn new(master_key: &[u8]) -> Result<Self> {
        let cipher = Aes256Gcm::new_from_slice(master_key)
            .map_err(|_| Error::Config("the local master key must be 32 bytes".into()))?;
        let fingerprint = blake3::derive_key("rill local key service fingerprint", master_key);
        Ok(LocalKeyService {
            cipher,
            fingerprint: hex::encode(&fingerprint[..8]),
        })
    }

    /// A service with a fresh random master key, for in-memory dev servers.
    pub fn generate() -> Self {
        Self::new(&rand::random::<[u8; 32]>()).expect("32 bytes is a valid AES-256 key")
    }

    fn aad(&self) -> Vec<u8> {
        format!("rill:token-key:{BACKEND}:{}", self.fingerprint).into_bytes()
    }
}

#[async_trait]
impl KeyWrapper for LocalKeyService {
    fn backend(&self) -> &'static str {
        BACKEND
    }

    fn key_ref(&self) -> &str {
        &self.fingerprint
    }

    async fn wrap(&self, plaintext: &[u8]) -> Result<WrappedSecret> {
        let nonce: [u8; NONCE_LEN] = rand::random();
        let aad = self.aad();
        let sealed = self
            .cipher
            .encrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|e| Error::key_service(BACKEND, e))?;
        let mut ciphertext = nonce.to_vec();
        ciphertext.extend_from_slice(&sealed);
        Ok(WrappedSecret {
            backend: BACKEND.into(),
            key_ref: self.fingerprint.clone(),
            ciphertext,
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
        if wrapped.ciphertext.len() < NONCE_LEN {
            return Err(Error::key_service(BACKEND, "wrapped secret is truncated"));
        }
        let (nonce, sealed) = wrapped.ciphertext.split_at(NONCE_LEN);
        let nonce: [u8; NONCE_LEN] = nonce.try_into().expect("split at NONCE_LEN");
        let aad = self.aad();
        self.cipher
            .decrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: sealed,
                    aad: &aad,
                },
            )
            .map_err(|_| Error::key_service(BACKEND, "wrapped secret failed authentication"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn wrapped_secrets_open_only_under_their_master_key() {
        let service = LocalKeyService::new(&[7; 32]).unwrap();
        let wrapped = service.wrap(b"secret").await.unwrap();
        assert_ne!(wrapped.ciphertext, b"secret");
        assert_eq!(service.unwrap(&wrapped).await.unwrap(), b"secret");

        let other = LocalKeyService::new(&[8; 32]).unwrap();
        assert!(other.unwrap(&wrapped).await.is_err());

        let mut tampered = wrapped.clone();
        let last = tampered.ciphertext.len() - 1;
        tampered.ciphertext[last] ^= 1;
        assert!(service.unwrap(&tampered).await.is_err());
        assert!(LocalKeyService::new(&[1; 16]).is_err());
    }
}
