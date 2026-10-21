//! Creating, loading and rotating token signing keys.
//!
//! A new key is stored with an activation time in the future, so every
//! server learns its public half (through the identity log) before any
//! token signed by it arrives. The key it replaces keeps verifying until
//! every token it can have signed has expired, then retires.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rill_policy::grants::{GrantKeys, GrantSigner};
use rill_policy::{Issuer, KeyRing, SigningKey, TokenLifetimes};
use rill_store::identity::IdentityStore;
use rill_types::identity::TokenKeyRecord;
use rill_types::{Principal, Timestamp};

use super::wrapper::KeyWrapper;
use crate::error::{Error, Result};

/// When keys rotate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RotationPolicy {
    /// A key is replaced this long after it was created.
    pub rotate_after_ms: i64,
    /// A new key signs only this long after it is stored.
    pub activation_delay_ms: i64,
    /// Extra time an old key verifies past the last token it signed.
    pub grace_ms: i64,
    /// Retired keys are deleted this long after retiring.
    pub keep_retired_ms: i64,
}

impl Default for RotationPolicy {
    fn default() -> Self {
        RotationPolicy {
            rotate_after_ms: 30 * 86_400_000,
            activation_delay_ms: 60_000,
            grace_ms: 300_000,
            keep_retired_ms: 86_400_000,
        }
    }
}

/// The keys a server verifies with and the issuer it signs with.
#[derive(Debug, Clone)]
pub struct KeySet {
    pub ring: KeyRing,
    /// `None` while no key this server can unwrap is active.
    pub issuer: Option<Arc<Issuer>>,
    /// Public grant keys of every stored key.
    pub grant_keys: GrantKeys,
    /// Signs grants with the key derived from the issuer's.
    pub grant_signer: Option<GrantSigner>,
    /// When a stored key next becomes active; reload then.
    pub next_activation: Option<Timestamp>,
}

/// Token keys over the identity store and a key service.
pub struct KeyManager {
    identity: IdentityStore,
    wrapper: Arc<dyn KeyWrapper>,
    policy: RotationPolicy,
    lifetimes: TokenLifetimes,
    secrets: Mutex<HashMap<u32, Vec<u8>>>,
}

impl KeyManager {
    pub fn new(
        identity: IdentityStore,
        wrapper: Arc<dyn KeyWrapper>,
        policy: RotationPolicy,
        lifetimes: TokenLifetimes,
    ) -> Self {
        KeyManager {
            identity,
            wrapper,
            policy,
            lifetimes,
            secrets: Mutex::new(HashMap::new()),
        }
    }

    pub fn policy(&self) -> &RotationPolicy {
        &self.policy
    }

    /// The key service, which also wraps other secrets the instance keeps
    /// (identity provider client secrets).
    pub fn key_service(&self) -> &Arc<dyn KeyWrapper> {
        &self.wrapper
    }

    /// The longest any token lives: how long a key must outlive its last
    /// signature.
    fn longest_token_ms(&self) -> i64 {
        let l = &self.lifetimes;
        l.human_ms.max(l.agent_ms).max(l.service_ms)
    }

    /// Make sure a key this server can sign with exists, creating one that
    /// is active at once when none does (a fresh instance, or a key
    /// service that changed and cannot open the stored keys).
    pub async fn ensure_key(&self, now: Timestamp, actor: &Principal) -> Result<()> {
        let keys = self.identity.token_keys().await?;
        let usable = keys
            .iter()
            .any(|k| k.not_after.is_none() && self.wrapper.can_unwrap(&k.secret));
        if usable {
            return Ok(());
        }
        if !keys.is_empty() {
            tracing::warn!(
                backend = self.wrapper.backend(),
                "no stored token key opens with this key service; adding one"
            );
        }
        self.add_key(&keys, now, now, actor).await.map(|_| ())
    }

    /// Store a new key that signs from `active_from`, retiring the keys it
    /// replaces once their tokens have expired. Returns the new key's id,
    /// or `None` when another server added the same id first.
    async fn add_key(
        &self,
        existing: &[TokenKeyRecord],
        now: Timestamp,
        active_from: Timestamp,
        actor: &Principal,
    ) -> Result<Option<u32>> {
        let id = existing.iter().map(|k| k.id + 1).max().unwrap_or(1);
        let key = SigningKey::generate(id);
        let secret = self.wrapper.wrap(&key.secret_bytes()).await?;
        let record = TokenKeyRecord {
            id,
            public_key: key.public_key_bytes(),
            grant_public_key: GrantSigner::derive(id, &key.secret_bytes())?.public_key(),
            secret,
            created_at: now,
            active_from,
            not_after: None,
        };
        match self.identity.add_token_key(&record, actor).await {
            Ok(()) => {}
            Err(rill_store::Error::Conflict(_)) => return Ok(None),
            Err(e) => return Err(e.into()),
        }
        self.cache(id, key.secret_bytes());
        let retire = Timestamp::from_millis(
            active_from.millis() + self.longest_token_ms() + self.policy.grace_ms,
        );
        for old in existing.iter().filter(|k| k.not_after.is_none()) {
            self.identity
                .retire_token_key(old.id, retire, actor)
                .await?;
        }
        tracing::info!(key = id, active_from = %active_from, "token signing key added");
        Ok(Some(id))
    }

    /// Replace the signing key now (it activates after the activation
    /// delay). Returns the new key's id.
    pub async fn rotate(&self, now: Timestamp, actor: &Principal) -> Result<Option<u32>> {
        let keys = self.identity.token_keys().await?;
        let active_from = Timestamp::from_millis(now.millis() + self.policy.activation_delay_ms);
        self.add_key(&keys, now, active_from, actor).await
    }

    /// Rotate when the newest key is older than the rotation period, and
    /// delete keys retired long enough ago. Returns whether it rotated.
    pub async fn maintain(&self, now: Timestamp, actor: &Principal) -> Result<bool> {
        let keys = self.identity.token_keys().await?;
        let before = Timestamp::from_millis(now.millis() - self.policy.keep_retired_ms);
        self.identity.prune_token_keys(before, actor).await?;
        let due = keys.iter().max_by_key(|k| k.id).is_some_and(|newest| {
            now.millis() - newest.created_at.millis() >= self.policy.rotate_after_ms
        });
        if due {
            self.rotate(now, actor).await?;
        }
        Ok(due)
    }

    /// The keys to verify with at `now` and the issuer to sign with.
    pub async fn load(&self, now: Timestamp) -> Result<KeySet> {
        let keys = self.identity.token_keys().await?;
        let mut ring = KeyRing::new();
        let mut grant_keys = GrantKeys::new();
        for key in &keys {
            ring.insert_bytes(key.id, &key.public_key, key.not_after)?;
            grant_keys.insert(key.id, &key.grant_public_key, key.not_after)?;
        }
        let signer = keys
            .iter()
            .filter(|k| k.active_from <= now && k.not_after.is_none_or(|t| now < t))
            .filter(|k| self.wrapper.can_unwrap(&k.secret))
            .max_by_key(|k| (k.active_from, k.id));
        let (issuer, grant_signer) = match signer {
            Some(key) => {
                let secret = self.secret(key).await?;
                let signing = SigningKey::from_secret_bytes(key.id, &secret)?;
                let grants = GrantSigner::derive(key.id, &secret)?;
                if signing.public_key_bytes() != key.public_key
                    || grants.public_key() != key.grant_public_key
                {
                    return Err(Error::Config(format!(
                        "token key {} does not match its public keys",
                        key.id
                    )));
                }
                (
                    Some(Arc::new(Issuer::new(signing, self.lifetimes))),
                    Some(grants),
                )
            }
            None => (None, None),
        };
        let next_activation = keys
            .iter()
            .filter(|k| k.active_from > now)
            .map(|k| k.active_from)
            .min();
        Ok(KeySet {
            ring,
            issuer,
            grant_keys,
            grant_signer,
            next_activation,
        })
    }

    fn cache(&self, id: u32, secret: Vec<u8>) {
        self.secrets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, secret);
    }

    async fn secret(&self, key: &TokenKeyRecord) -> Result<Vec<u8>> {
        if let Some(secret) = self
            .secrets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key.id)
        {
            return Ok(secret.clone());
        }
        let secret = self.wrapper.unwrap(&key.secret).await?;
        self.cache(key.id, secret.clone());
        Ok(secret)
    }
}
