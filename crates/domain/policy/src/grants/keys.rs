//! Signing and verifying grants. A grant key is derived from a token
//! signing key, so it is wrapped by the same key service, carries the same
//! id, and rotates and retires with it.

use std::collections::HashMap;

use p256::ecdsa::signature::{Signer as _, Verifier as _};
use p256::ecdsa::{Signature, SigningKey, VerifyingKey};
use rill_types::{Principal, RepoId, Timestamp};

use super::grant::{message, GrantEntry, ReadGrant, MAX_GRANT_TTL_MS};
use crate::error::{Error, Result};

const DERIVATION: &str = "rill 2026 read grant signing key";

/// Signs grants with the key derived from one token key.
#[derive(Clone)]
pub struct GrantSigner {
    key_id: u32,
    key: SigningKey,
}

impl std::fmt::Debug for GrantSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GrantSigner")
            .field("key_id", &self.key_id)
            .finish_non_exhaustive()
    }
}

impl GrantSigner {
    /// The grant key of the token key `key_id` with secret `token_secret`.
    pub fn derive(key_id: u32, token_secret: &[u8]) -> Result<Self> {
        Ok(GrantSigner {
            key_id,
            key: crate::signing::derive_key(DERIVATION, token_secret)?,
        })
    }

    pub fn key_id(&self) -> u32 {
        self.key_id
    }

    /// The compressed public key verifiers need.
    pub fn public_key(&self) -> Vec<u8> {
        self.key
            .verifying_key()
            .to_encoded_point(true)
            .as_bytes()
            .to_vec()
    }

    /// A grant letting `principal` fetch `entries` of `repo` for `ttl_ms`
    /// (at most [`MAX_GRANT_TTL_MS`]).
    pub fn sign(
        &self,
        principal: &Principal,
        repo: RepoId,
        entries: Vec<GrantEntry>,
        now: Timestamp,
        ttl_ms: i64,
    ) -> Result<String> {
        let grant = ReadGrant {
            key_id: self.key_id,
            principal: principal.id,
            repo,
            expires_at: Timestamp::from_millis(now.millis() + ttl_ms.min(MAX_GRANT_TTL_MS)),
            entries,
        };
        let body = grant.body()?;
        let signature: Signature = self.key.sign(&message(&body));
        let signature = signature.normalize_s().unwrap_or(signature);
        ReadGrant::encode(body, signature.to_bytes().to_vec())
    }
}

/// The public grant keys, by id, with when each stops verifying.
#[derive(Debug, Clone, Default)]
pub struct GrantKeys {
    keys: HashMap<u32, (VerifyingKey, Option<Timestamp>)>,
}

impl GrantKeys {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add key `id` from its compressed public key.
    pub fn insert(
        &mut self,
        id: u32,
        public_key: &[u8],
        not_after: Option<Timestamp>,
    ) -> Result<()> {
        let key = VerifyingKey::from_sec1_bytes(public_key)
            .map_err(|e| Error::InvalidKey(format!("grant key {id}: {e}")))?;
        self.keys.insert(id, (key, not_after));
        Ok(())
    }

    /// Key ids and public keys, for publishing.
    pub fn public_keys(&self) -> Vec<(u32, Vec<u8>, Option<Timestamp>)> {
        let mut out: Vec<_> = self
            .keys
            .iter()
            .map(|(id, (key, not_after))| {
                (
                    *id,
                    key.to_encoded_point(true).as_bytes().to_vec(),
                    *not_after,
                )
            })
            .collect();
        out.sort_by_key(|(id, _, _)| *id);
        out
    }

    /// Whether key `id` is known.
    pub fn contains(&self, id: u32) -> bool {
        self.keys.contains_key(&id)
    }

    /// The grant `text` says, once its key, signature and expiry check out.
    pub fn verify(&self, text: &str, now: Timestamp) -> Result<ReadGrant> {
        let (grant, body, signature) = ReadGrant::decode(text)?;
        let refuse = |why: &str| Error::TokenRejected(format!("read grant: {why}"));
        let (key, not_after) = self
            .keys
            .get(&grant.key_id)
            .ok_or_else(|| refuse("unknown key"))?;
        if not_after.is_some_and(|t| now >= t) {
            return Err(refuse("its key is retired"));
        }
        let signature = Signature::from_slice(&signature).map_err(|_| refuse("bad signature"))?;
        if signature.normalize_s().is_some() {
            return Err(refuse("bad signature"));
        }
        key.verify(&message(&body), &signature)
            .map_err(|_| refuse("bad signature"))?;
        if now >= grant.expires_at {
            return Err(refuse("expired"));
        }
        Ok(grant)
    }
}

#[cfg(test)]
mod tests {
    use rill_types::{ObjectHash, ObjectKind, RepoPathBuf};

    use super::*;

    fn entry(bytes: &[u8], path: &str) -> GrantEntry {
        GrantEntry {
            kind: ObjectKind::Blob,
            hash: ObjectHash::digest(ObjectKind::Blob, bytes),
            path: RepoPathBuf::new(path).unwrap(),
        }
    }

    #[test]
    fn grants_verify_only_unaltered_unexpired_and_under_known_keys() {
        let signer = GrantSigner::derive(7, &[42u8; 32]).unwrap();
        let mut keys = GrantKeys::new();
        keys.insert(7, &signer.public_key(), None).unwrap();
        let now = Timestamp::from_millis(1_000_000);
        let repo = RepoId::new();
        let ann = Principal::human("ann");
        let text = signer
            .sign(&ann, repo, vec![entry(b"a", "pub/a")], now, 60_000)
            .unwrap();
        let grant = keys.verify(&text, now).unwrap();
        assert_eq!(grant.principal, ann.id);
        assert!(grant
            .entry(
                ObjectKind::Blob,
                &ObjectHash::digest(ObjectKind::Blob, b"a")
            )
            .is_some());
        assert!(grant
            .entry(
                ObjectKind::Blob,
                &ObjectHash::digest(ObjectKind::Blob, b"b")
            )
            .is_none());

        assert!(
            keys.verify(&text, Timestamp::from_millis(now.millis() + 60_000))
                .is_err(),
            "expired"
        );
        let long = signer
            .sign(&ann, repo, vec![], now, 10 * MAX_GRANT_TTL_MS)
            .unwrap();
        assert!(
            keys.verify(
                &long,
                Timestamp::from_millis(now.millis() + MAX_GRANT_TTL_MS)
            )
            .is_err(),
            "lifetimes are capped"
        );

        // Altering the body (adding an entry) breaks the signature.
        let (mut forged, _, signature) = ReadGrant::decode(&text).unwrap();
        forged.entries.push(entry(b"secret", "secret/s"));
        let tampered = ReadGrant::encode(forged.body().unwrap(), signature).unwrap();
        assert!(keys.verify(&tampered, now).is_err());

        let other = GrantSigner::derive(8, &[43u8; 32]).unwrap();
        let foreign = other.sign(&ann, repo, vec![], now, 60_000).unwrap();
        assert!(keys.verify(&foreign, now).is_err(), "unknown key");
        let mut retired = GrantKeys::new();
        retired.insert(7, &signer.public_key(), Some(now)).unwrap();
        assert!(retired.verify(&text, now).is_err());
        assert!(keys.verify("garbage", now).is_err());
    }

    #[test]
    fn grant_keys_are_derived_deterministically_and_apart_from_the_token_key() {
        let a = GrantSigner::derive(1, &[9u8; 32]).unwrap();
        let b = GrantSigner::derive(1, &[9u8; 32]).unwrap();
        assert_eq!(
            a.public_key(),
            b.public_key(),
            "every server derives the same key"
        );
        let token = SigningKey::from_slice(&[9u8; 32]).unwrap();
        assert_ne!(
            a.public_key(),
            token
                .verifying_key()
                .to_encoded_point(true)
                .as_bytes()
                .to_vec()
        );
    }
}
