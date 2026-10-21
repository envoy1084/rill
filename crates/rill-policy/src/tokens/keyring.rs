//! The P-256 keys tokens are signed and verified with.
//!
//! Every token names the key that signed its authority block (Biscuit's
//! root key id), so several keys can be valid at once while one rotates
//! out: verifiers keep every key that has not retired.

use std::collections::BTreeMap;

use biscuit_auth::{Algorithm, KeyPair, PrivateKey, PublicKey};
use rill_types::Timestamp;

use crate::error::{Error, Result};

/// A signing key with its id.
pub struct SigningKey {
    pub id: u32,
    pub(crate) pair: KeyPair,
}

impl std::fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SigningKey")
            .field("id", &self.id)
            .field("public_key", &hex::encode(self.public_key_bytes()))
            .finish()
    }
}

impl SigningKey {
    /// A fresh random P-256 key.
    pub fn generate(id: u32) -> Self {
        SigningKey {
            id,
            pair: KeyPair::new_with_algorithm(Algorithm::Secp256r1),
        }
    }

    /// The key with secret scalar `bytes`.
    pub fn from_secret_bytes(id: u32, bytes: &[u8]) -> Result<Self> {
        let secret = PrivateKey::from_bytes(bytes, Algorithm::Secp256r1)
            .map_err(|e| Error::InvalidKey(e.to_string()))?;
        Ok(SigningKey {
            id,
            pair: KeyPair::from(&secret),
        })
    }

    /// The 32-byte secret scalar. Treat as secret.
    pub fn secret_bytes(&self) -> Vec<u8> {
        self.pair.private().to_bytes().to_vec()
    }

    pub fn public_key(&self) -> PublicKey {
        self.pair.public()
    }

    /// The compressed public key.
    pub fn public_key_bytes(&self) -> Vec<u8> {
        self.pair.public().to_bytes()
    }
}

/// A verifying key and when it stops being trusted.
#[derive(Debug, Clone)]
pub struct VerifyingKey {
    pub key: PublicKey,
    /// Tokens signed by this key are refused from this time on.
    pub not_after: Option<Timestamp>,
}

/// Public keys by id.
#[derive(Debug, Clone, Default)]
pub struct KeyRing {
    keys: BTreeMap<u32, VerifyingKey>,
}

impl KeyRing {
    pub fn new() -> Self {
        Self::default()
    }

    /// A ring holding `key` with no retirement date.
    pub fn single(id: u32, key: PublicKey) -> Self {
        let mut ring = KeyRing::new();
        ring.insert(id, key, None);
        ring
    }

    /// Trust `key` as `id` until `not_after`.
    pub fn insert(&mut self, id: u32, key: PublicKey, not_after: Option<Timestamp>) {
        self.keys.insert(id, VerifyingKey { key, not_after });
    }

    /// Trust the compressed P-256 key `bytes` as `id`.
    pub fn insert_bytes(
        &mut self,
        id: u32,
        bytes: &[u8],
        not_after: Option<Timestamp>,
    ) -> Result<()> {
        let key = PublicKey::from_bytes(bytes, Algorithm::Secp256r1)
            .map_err(|e| Error::InvalidKey(e.to_string()))?;
        self.insert(id, key, not_after);
        Ok(())
    }

    pub fn contains(&self, id: u32) -> bool {
        self.keys.contains_key(&id)
    }

    pub fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.keys.keys().copied()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// The key `id` names, if it is trusted at `now`.
    pub fn key_at(&self, id: Option<u32>, now: Timestamp) -> Result<PublicKey> {
        let id = id.ok_or_else(|| Error::Token("token names no signing key".into()))?;
        let entry = self
            .keys
            .get(&id)
            .ok_or_else(|| Error::Token(format!("unknown signing key {id}")))?;
        match entry.not_after {
            Some(end) if now >= end => Err(Error::Token(format!("signing key {id} has retired"))),
            _ => Ok(entry.key),
        }
    }
}
