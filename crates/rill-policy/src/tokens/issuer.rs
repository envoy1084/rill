//! Issuing and attenuating capability tokens.

use biscuit_auth::datalog::SymbolTable;
use biscuit_auth::{Algorithm, Biscuit, KeyPair, PublicKey};
use rill_types::{Principal, Timestamp};

use super::capability::Capability;
use super::claims::{TokenClaims, TokenLifetimes};
use super::datalog::{authority_block, capability_block, effective_scope};
use super::keyring::{KeyRing, SigningKey};
use crate::error::{Error, Result};

fn token_err(e: impl std::fmt::Display) -> Error {
    Error::Token(e.to_string())
}

/// A fresh P-256 key pair. Biscuit signs each block with the previous
/// block's ephemeral key and defaults those to Ed25519; every key a Rill
/// token uses is P-256.
pub(crate) fn ephemeral_key() -> KeyPair {
    KeyPair::new_with_algorithm(Algorithm::Secp256r1)
}

/// Signs tokens with the current key, refusing lifetimes longer than a
/// principal's kind allows.
pub struct Issuer {
    key: SigningKey,
    lifetimes: TokenLifetimes,
}

impl std::fmt::Debug for Issuer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Issuer").field("key", &self.key).finish()
    }
}

impl Issuer {
    /// An issuer signing with `key`.
    pub fn new(key: SigningKey, lifetimes: TokenLifetimes) -> Self {
        Issuer { key, lifetimes }
    }

    /// An issuer with a fresh key `0` and the default lifetimes.
    pub fn generate() -> Self {
        Issuer::new(SigningKey::generate(0), TokenLifetimes::default())
    }

    pub fn key_id(&self) -> u32 {
        self.key.id
    }

    /// The public key verifiers need.
    pub fn public_key(&self) -> PublicKey {
        self.key.public_key()
    }

    /// A ring trusting only this issuer's key.
    pub fn key_ring(&self) -> KeyRing {
        KeyRing::single(self.key.id, self.public_key())
    }

    pub fn lifetimes(&self) -> &TokenLifetimes {
        &self.lifetimes
    }

    /// Issue a token for `principal` with `claims`, narrowed to
    /// `capability` when one is given, as base64.
    pub fn issue(
        &self,
        principal: &Principal,
        claims: &TokenClaims,
        capability: Option<&Capability>,
    ) -> Result<String> {
        self.lifetimes.check(principal, claims)?;
        let block = authority_block(principal, claims, capability)?;
        let token = Biscuit::builder()
            .root_key_id(self.key.id)
            .merge(block)
            .build_with_key_pair(&self.key.pair, SymbolTable::default(), &ephemeral_key())
            .map_err(token_err)?;
        token.to_base64().map_err(token_err)
    }
}

/// Derive a child token that grants no more than `token` does.
///
/// Needs the trusted keys to read the parent's scope from the verified
/// token; the holder then appends a block with the narrower checks.
/// Biscuit checks accumulate, so even a client that bypasses the narrowing
/// check here cannot produce a token that passes verification for anything
/// the parent forbade.
pub fn attenuate(token: &str, keys: &KeyRing, narrower: &Capability) -> Result<String> {
    let now = Timestamp::now();
    let parent = Biscuit::from_base64(token, |id| keys.key_at(id, now).map_err(key_err))
        .map_err(token_err)?;
    let (claims, effective) = effective_scope(&parent)?;
    match &effective {
        Some(parent) => narrower.check_narrower_than(parent)?,
        None if narrower.expires_at > claims.expires_at => {
            return Err(Error::Widening("the capability outlives the token".into()))
        }
        None => narrower.validate()?,
    }
    let child = parent
        .append_with_keypair(&ephemeral_key(), capability_block(narrower)?)
        .map_err(token_err)?;
    child.to_base64().map_err(token_err)
}

/// Read a token's claims and effective capability without authorizing a
/// request.
pub fn inspect(token: &str, keys: &KeyRing) -> Result<(TokenClaims, Option<Capability>)> {
    inspect_at(token, keys, Timestamp::now())
}

/// [`inspect`] with key retirement judged at `now`.
pub(crate) fn inspect_at(
    token: &str,
    keys: &KeyRing,
    now: Timestamp,
) -> Result<(TokenClaims, Option<Capability>)> {
    let token = Biscuit::from_base64(token, |id| keys.key_at(id, now).map_err(key_err))
        .map_err(token_err)?;
    effective_scope(&token)
}

/// Biscuit's key provider must answer with its own error type.
pub(crate) fn key_err(e: Error) -> biscuit_auth::error::Format {
    tracing::debug!(error = %e, "token signing key refused");
    biscuit_auth::error::Format::UnknownPublicKey
}
