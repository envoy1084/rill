//! AES-256-GCM sealing under per-org data keys.
//!
//! A sealed frame is `RXE1 ‖ key id ‖ nonce ‖ ciphertext ‖ tag`. Each object
//! is encrypted under its own key, derived from the org's data key and the
//! object's kind and hash, so nonces never need counting. The associated
//! data binds the frame to its org, kind, hash and key id: a frame copied to
//! another org, or under another hash, does not open.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use async_trait::async_trait;
use parking_lot::RwLock;
use rill_types::Timestamp;

use super::sealer::{ObjectAt, Sealer};
use crate::error::{Error, Result};

const MAGIC: &[u8; 4] = b"RXE1";
const NONCE_LEN: usize = 12;
const HEADER_LEN: usize = MAGIC.len() + 4 + NONCE_LEN;

/// How long an org's keys are used before they are read again. New keys
/// activate later than this, so every server holds a key before any object
/// is sealed with it.
pub const KEY_REFRESH: Duration = Duration::from_secs(60);

/// One unwrapped data key.
#[derive(Clone)]
pub struct DataKey {
    pub id: u32,
    pub secret: [u8; 32],
    pub active_from: Timestamp,
}

impl std::fmt::Debug for DataKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataKey")
            .field("id", &self.id)
            .field("active_from", &self.active_from)
            .finish_non_exhaustive()
    }
}

/// Where an [`OrgSealer`] gets an org's data keys.
#[async_trait]
pub trait DataKeySource: Send + Sync {
    /// Every data key of `org`, creating its first when it has none.
    async fn data_keys(&self, org: &str) -> Result<Vec<DataKey>>;
}

struct OrgKeys {
    keys: Vec<DataKey>,
    loaded_at: Instant,
}

impl OrgKeys {
    fn get(&self, id: u32) -> Option<&DataKey> {
        self.keys.iter().find(|k| k.id == id)
    }

    fn sealing(&self, now: Timestamp) -> Option<&DataKey> {
        self.keys
            .iter()
            .filter(|k| k.active_from <= now)
            .max_by_key(|k| (k.active_from, k.id))
    }
}

/// Seals each org's objects under that org's data keys.
#[derive(Default)]
pub struct OrgSealer {
    source: OnceLock<Arc<dyn DataKeySource>>,
    orgs: RwLock<HashMap<String, Arc<OrgKeys>>>,
}

impl std::fmt::Debug for OrgSealer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrgSealer").finish_non_exhaustive()
    }
}

impl OrgSealer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set where keys come from; objects neither seal nor open until then.
    pub fn set_source(&self, source: Arc<dyn DataKeySource>) {
        if self.source.set(source).is_err() {
            tracing::warn!("data key source set twice; keeping the first");
        }
    }

    /// Forget `org`'s keys, so the next object reads them again.
    pub fn forget(&self, org: &str) {
        self.orgs.write().remove(org);
    }

    fn keys(&self, org: &str) -> Result<Arc<OrgKeys>> {
        self.orgs
            .read()
            .get(org)
            .cloned()
            .ok_or_else(|| Error::Sealed(format!("the data keys of {org} are not loaded")))
    }

    fn cipher(key: &DataKey, at: &ObjectAt<'_>) -> Aes256Gcm {
        let mut derive = blake3::Hasher::new_keyed(&key.secret);
        derive.update(b"rill.v1 object key");
        derive.update(&[at.kind as u8]);
        derive.update(at.hash.as_bytes());
        Aes256Gcm::new(derive.finalize().as_bytes().into())
    }

    fn aad(at: &ObjectAt<'_>, key_id: u32) -> Vec<u8> {
        let mut aad = b"rill.v1 sealed object".to_vec();
        aad.extend_from_slice(&(at.namespace.len() as u32).to_be_bytes());
        aad.extend_from_slice(at.namespace.as_bytes());
        aad.push(at.kind as u8);
        aad.extend_from_slice(at.hash.as_bytes());
        aad.extend_from_slice(&key_id.to_be_bytes());
        aad
    }
}

#[async_trait]
impl Sealer for OrgSealer {
    async fn ready(&self, namespace: &str) -> Result<()> {
        if let Some(keys) = self.orgs.read().get(namespace) {
            if keys.loaded_at.elapsed() < KEY_REFRESH {
                return Ok(());
            }
        }
        let source = self
            .source
            .get()
            .ok_or_else(|| Error::Sealed("data keys are not available yet".into()))?;
        let keys = source.data_keys(namespace).await?;
        if keys.is_empty() {
            return Err(Error::Sealed(format!("{namespace} has no data key")));
        }
        self.orgs.write().insert(
            namespace.to_string(),
            Arc::new(OrgKeys {
                keys,
                loaded_at: Instant::now(),
            }),
        );
        Ok(())
    }

    fn seal(&self, at: ObjectAt<'_>, frame: Vec<u8>) -> Result<Vec<u8>> {
        let keys = self.keys(at.namespace)?;
        let key = keys
            .sealing(Timestamp::now())
            .ok_or_else(|| Error::Sealed(format!("{} has no active data key", at.namespace)))?;
        let nonce: [u8; NONCE_LEN] = rand::random();
        let aad = Self::aad(&at, key.id);
        let sealed = Self::cipher(key, &at)
            .encrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: &frame,
                    aad: &aad,
                },
            )
            .map_err(|_| Error::Sealed("encryption failed".into()))?;
        let mut out = Vec::with_capacity(HEADER_LEN + sealed.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&key.id.to_be_bytes());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&sealed);
        Ok(out)
    }

    fn open(&self, at: ObjectAt<'_>, stored: Vec<u8>) -> Result<Vec<u8>> {
        let refuse = |why: &str| Error::Sealed(format!("{} {}: {why}", at.kind, at.hash));
        if stored.len() < HEADER_LEN || &stored[..MAGIC.len()] != MAGIC {
            return Err(refuse("not sealed"));
        }
        let (id, rest) = stored[MAGIC.len()..].split_at(4);
        let key_id = u32::from_be_bytes(id.try_into().expect("split at 4"));
        let (nonce, sealed) = rest.split_at(NONCE_LEN);
        let nonce: [u8; NONCE_LEN] = nonce.try_into().expect("split at NONCE_LEN");
        let keys = self.keys(at.namespace)?;
        let key = keys.get(key_id).ok_or_else(|| {
            refuse(&format!(
                "sealed with data key {key_id}, which it does not have"
            ))
        })?;
        let aad = Self::aad(&at, key_id);
        Self::cipher(key, &at)
            .decrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: sealed,
                    aad: &aad,
                },
            )
            .map_err(|_| refuse("failed authentication"))
    }
}

#[cfg(test)]
mod tests {
    use rill_types::{ObjectHash, ObjectKind};

    use super::*;

    struct Fixed(HashMap<String, Vec<DataKey>>);

    #[async_trait]
    impl DataKeySource for Fixed {
        async fn data_keys(&self, org: &str) -> Result<Vec<DataKey>> {
            Ok(self.0.get(org).cloned().unwrap_or_default())
        }
    }

    fn key(id: u32, byte: u8, active_from: i64) -> DataKey {
        DataKey {
            id,
            secret: [byte; 32],
            active_from: Timestamp::from_millis(active_from),
        }
    }

    async fn sealer(keys: &[(&str, Vec<DataKey>)]) -> OrgSealer {
        let sealer = OrgSealer::new();
        sealer.set_source(Arc::new(Fixed(
            keys.iter()
                .map(|(o, k)| (o.to_string(), k.clone()))
                .collect(),
        )));
        for (org, _) in keys {
            sealer.ready(org).await.unwrap();
        }
        sealer
    }

    #[tokio::test]
    async fn a_frame_opens_only_as_the_object_and_org_it_was_sealed_for() {
        let sealer = sealer(&[("acme", vec![key(1, 1, 0)]), ("globex", vec![key(1, 2, 0)])]).await;
        let plain = b"fn main() { let password = 42; }".to_vec();
        let hash = ObjectHash::digest(ObjectKind::Blob, &plain);
        let at = |namespace| ObjectAt {
            namespace,
            kind: ObjectKind::Blob,
            hash: &hash,
        };
        let stored = sealer.seal(at("acme"), plain.clone()).unwrap();
        assert!(!stored.windows(8).any(|w| w == b"password"));
        assert_eq!(sealer.open(at("acme"), stored.clone()).unwrap(), plain);

        assert!(
            sealer.open(at("globex"), stored.clone()).is_err(),
            "another org"
        );
        let other = ObjectHash::digest(ObjectKind::Blob, b"other");
        let elsewhere = ObjectAt {
            namespace: "acme",
            kind: ObjectKind::Blob,
            hash: &other,
        };
        assert!(
            sealer.open(elsewhere, stored.clone()).is_err(),
            "another hash"
        );
        let mut tampered = stored.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert!(sealer.open(at("acme"), tampered).is_err());
        assert!(
            sealer.open(at("acme"), plain).is_err(),
            "plaintext is refused"
        );
        assert!(
            sealer.open(at("initech"), stored).is_err(),
            "keys never loaded"
        );
    }

    #[tokio::test]
    async fn new_keys_seal_once_active_and_old_ones_still_open() {
        let later = Timestamp::now().millis() + 3_600_000;
        let sealer = sealer(&[("acme", vec![key(1, 1, 0), key(2, 9, later)])]).await;
        let hash = ObjectHash::digest(ObjectKind::Tree, b"t");
        let at = ObjectAt {
            namespace: "acme",
            kind: ObjectKind::Tree,
            hash: &hash,
        };
        let stored = sealer.seal(at, b"t".to_vec()).unwrap();
        assert_eq!(&stored[4..8], &1u32.to_be_bytes(), "the pending key waits");

        let rotated = super::OrgSealer::new();
        rotated.set_source(Arc::new(Fixed(HashMap::from([(
            "acme".to_string(),
            vec![key(1, 1, 0), key(2, 9, 0)],
        )]))));
        rotated.ready("acme").await.unwrap();
        let newer = rotated.seal(at, b"t".to_vec()).unwrap();
        assert_eq!(&newer[4..8], &2u32.to_be_bytes());
        assert_eq!(rotated.open(at, stored).unwrap(), b"t");
    }
}
