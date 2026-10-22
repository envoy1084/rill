//! Each org's data keys: 32 random bytes per key, wrapped by the key
//! service, unwrapped once per process.
//!
//! An org gets its first key, active at once, the first time one of its
//! objects is sealed. A rotated key activates after a delay longer than
//! [`KEY_REFRESH`], so every server holds it before anything is sealed
//! with it. Old keys stay to open what they sealed.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rill_store::identity::IdentityStore;
use rill_store::{DataKey, DataKeySource, KEY_REFRESH};
use rill_types::identity::DataKeyRecord;
use rill_types::{Principal, Timestamp};

use super::wrapper::KeyWrapper;
use crate::error::{Error, Result};

/// How long after it is stored a rotated data key starts sealing.
pub const DATA_KEY_ACTIVATION_MS: i64 = 10 * KEY_REFRESH.as_millis() as i64;

/// Data keys over the identity store and a key service.
pub struct DataKeyManager {
    identity: IdentityStore,
    wrapper: Arc<dyn KeyWrapper>,
    actor: Principal,
    secrets: Mutex<HashMap<(String, u32), [u8; 32]>>,
}

impl DataKeyManager {
    /// Keys it creates are recorded as made by `actor`.
    pub fn new(identity: IdentityStore, wrapper: Arc<dyn KeyWrapper>, actor: Principal) -> Self {
        DataKeyManager {
            identity,
            wrapper,
            actor,
            secrets: Mutex::new(HashMap::new()),
        }
    }

    async fn add(
        &self,
        org: &str,
        existing: &[DataKeyRecord],
        active_from: Timestamp,
    ) -> Result<Option<u32>> {
        let id = existing.iter().map(|k| k.id + 1).max().unwrap_or(1);
        let secret: [u8; 32] = rand::random();
        let record = DataKeyRecord {
            org: org.to_string(),
            id,
            secret: self.wrapper.wrap(&secret).await?,
            created_at: Timestamp::now(),
            active_from,
        };
        match self.identity.add_data_key(&record, &self.actor).await {
            Ok(()) => {}
            Err(rill_store::Error::Conflict(_)) => return Ok(None),
            Err(e) => return Err(e.into()),
        }
        self.cache(org, id, secret);
        tracing::info!(org, key = id, active_from = %active_from, "data key added");
        Ok(Some(id))
    }

    /// Add a key to `org` that seals from the activation delay on.
    /// Returns its id, or `None` when another server added one first.
    pub async fn rotate(&self, org: &str) -> Result<Option<u32>> {
        let existing = self.identity.data_keys(org).await?;
        let now = Timestamp::now();
        self.add(
            org,
            &existing,
            Timestamp::from_millis(now.millis() + DATA_KEY_ACTIVATION_MS),
        )
        .await
    }

    /// Wrap every data key again with the key service's current key, so
    /// none depends on an older one. Returns how many were rewrapped.
    pub async fn rewrap(&self) -> Result<usize> {
        let mut done = 0;
        for record in self.identity.all_data_keys().await? {
            let secret = self.wrapper.unwrap(&record.secret).await?;
            let rewrapped = DataKeyRecord {
                secret: self.wrapper.wrap(&secret).await?,
                ..record.clone()
            };
            self.identity
                .rewrap_data_key(&record, &rewrapped, &self.actor)
                .await?;
            done += 1;
        }
        Ok(done)
    }

    async fn records(&self, org: &str) -> Result<Vec<DataKeyRecord>> {
        let keys = self.identity.data_keys(org).await?;
        if !keys.is_empty() {
            return Ok(keys);
        }
        self.add(org, &keys, Timestamp::now()).await?;
        Ok(self.identity.data_keys(org).await?)
    }

    fn cache(&self, org: &str, id: u32, secret: [u8; 32]) {
        self.secrets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert((org.to_string(), id), secret);
    }

    async fn secret(&self, record: &DataKeyRecord) -> Result<[u8; 32]> {
        let cached = self
            .secrets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&(record.org.clone(), record.id))
            .copied();
        if let Some(secret) = cached {
            return Ok(secret);
        }
        let secret: [u8; 32] = self
            .wrapper
            .unwrap(&record.secret)
            .await?
            .try_into()
            .map_err(|_| {
                Error::Config(format!(
                    "data key {} of {} is not 32 bytes",
                    record.id, record.org
                ))
            })?;
        self.cache(&record.org, record.id, secret);
        Ok(secret)
    }

    async fn unwrapped(&self, org: &str) -> Result<Vec<DataKey>> {
        let mut keys = Vec::new();
        for record in self.records(org).await? {
            if !self.wrapper.can_unwrap(&record.secret) {
                tracing::warn!(
                    org,
                    key = record.id,
                    "data key does not open with this key service"
                );
                continue;
            }
            keys.push(DataKey {
                id: record.id,
                secret: self.secret(&record).await?,
                active_from: record.active_from,
            });
        }
        Ok(keys)
    }
}

#[async_trait]
impl DataKeySource for DataKeyManager {
    async fn data_keys(&self, org: &str) -> rill_store::Result<Vec<DataKey>> {
        self.unwrapped(org).await.map_err(|e| match e {
            Error::Store(e) => e,
            other => rill_store::Error::Sealed(other.to_string()),
        })
    }
}
