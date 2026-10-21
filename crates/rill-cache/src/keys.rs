//! The origin's grant keys: read at start, reread on a schedule, and early
//! when a grant names a key the cache has not seen (at most once a second).

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock};
use rill_policy::grants::{GrantKeys, ReadGrant};
use rill_types::Timestamp;
use serde::Deserialize;

use crate::error::{Error, Result};

const UNKNOWN_KEY_REFETCH: Duration = Duration::from_secs(1);

#[derive(Deserialize)]
struct Published {
    keys: Vec<PublishedKey>,
}

#[derive(Deserialize)]
struct PublishedKey {
    id: u32,
    public_key: String,
    not_after: Option<i64>,
}

/// The keys grants verify with.
pub struct OriginKeys {
    url: String,
    client: reqwest::Client,
    keys: RwLock<GrantKeys>,
    last_fetch: Mutex<Option<Instant>>,
}

impl OriginKeys {
    /// Fetch the keys `origin` publishes.
    pub async fn fetch(origin: &str, client: reqwest::Client) -> Result<Arc<Self>> {
        let keys = Arc::new(OriginKeys {
            url: format!("{}/grant-keys", origin.trim_end_matches('/')),
            client,
            keys: RwLock::new(GrantKeys::new()),
            last_fetch: Mutex::new(None),
        });
        keys.refresh().await?;
        Ok(keys)
    }

    /// Reread the keys.
    pub async fn refresh(&self) -> Result<()> {
        *self.last_fetch.lock() = Some(Instant::now());
        let published: Published = self
            .client
            .get(&self.url)
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| Error::Origin(format!("grant keys: {e}")))?
            .json()
            .await
            .map_err(|e| Error::Origin(format!("grant keys: {e}")))?;
        let mut keys = GrantKeys::new();
        for k in published.keys {
            let public = hex::decode(&k.public_key)
                .map_err(|e| Error::Origin(format!("grant key {}: {e}", k.id)))?;
            keys.insert(k.id, &public, k.not_after.map(Timestamp::from_millis))
                .map_err(|e| Error::Origin(e.to_string()))?;
        }
        *self.keys.write() = keys;
        Ok(())
    }

    /// The grant `text` says, once verified.
    pub async fn verify(&self, text: &str) -> Result<ReadGrant> {
        let unknown = ReadGrant::key_id_of(text).is_some_and(|id| !self.keys.read().contains(id));
        let due = self
            .last_fetch
            .lock()
            .is_none_or(|t| t.elapsed() >= UNKNOWN_KEY_REFETCH);
        if unknown && due {
            if let Err(e) = self.refresh().await {
                tracing::warn!(error = %e, "refetching grant keys failed");
            }
        }
        self.keys
            .read()
            .verify(text, Timestamp::now())
            .map_err(|e| Error::Denied(e.to_string()))
    }
}
