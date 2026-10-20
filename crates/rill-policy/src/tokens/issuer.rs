//! Issuing and attenuating capability tokens.

use biscuit_auth::{Biscuit, KeyPair, PrivateKey, PublicKey};
use rill_types::Principal;

use super::capability::Capability;
use super::datalog::{authority_block, capability_block, effective_capability};
use crate::error::{Error, Result};

fn token_err(e: impl std::fmt::Display) -> Error {
    Error::Token(e.to_string())
}

/// Holder of the root keypair that signs authority blocks.
pub struct Issuer {
    root: KeyPair,
}

impl std::fmt::Debug for Issuer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Issuer")
            .field("public_key", &self.public_key().to_bytes_hex())
            .finish()
    }
}

impl Issuer {
    /// Generate a fresh root keypair.
    pub fn generate() -> Self {
        Issuer {
            root: KeyPair::new(),
        }
    }

    /// Build from a 32-byte Ed25519 private key.
    pub fn from_private_key_bytes(bytes: &[u8]) -> Result<Self> {
        let key = PrivateKey::from_bytes(bytes, biscuit_auth::builder::Algorithm::Ed25519)
            .map_err(|e| Error::InvalidKey(e.to_string()))?;
        Ok(Issuer {
            root: KeyPair::from(&key),
        })
    }

    /// The private key bytes, for persisting.
    pub fn private_key_bytes(&self) -> Vec<u8> {
        self.root.private().to_bytes().to_vec()
    }

    /// The root public key verifiers need.
    pub fn public_key(&self) -> PublicKey {
        self.root.public()
    }

    /// Issue a token for `principal` with `capability`, as base64.
    pub fn issue(&self, principal: &Principal, capability: &Capability) -> Result<String> {
        let block = authority_block(principal, capability)?;
        let token = Biscuit::builder()
            .merge(block)
            .build(&self.root)
            .map_err(token_err)?;
        token.to_base64().map_err(token_err)
    }
}

/// Derive a child token that grants no more than `token` does.
///
/// Needs the root public key to read the parent's capability from the
/// verified token; the holder then appends a block with the narrower
/// checks. Biscuit checks accumulate, so even a client that bypasses the
/// narrowing check here cannot produce a token that passes verification
/// for anything the parent forbade.
pub fn attenuate(token: &str, root: &PublicKey, narrower: &Capability) -> Result<String> {
    let parent = Biscuit::from_base64(token, root).map_err(token_err)?;
    let effective = effective_capability(&parent)?;
    narrower.check_narrower_than(&effective)?;
    let child = parent
        .append(capability_block(narrower)?)
        .map_err(token_err)?;
    child.to_base64().map_err(token_err)
}

/// Read the effective capability of a token without authorizing a request.
pub fn inspect(token: &str, root: &PublicKey) -> Result<Capability> {
    let token = Biscuit::from_base64(token, root).map_err(token_err)?;
    effective_capability(&token)
}
