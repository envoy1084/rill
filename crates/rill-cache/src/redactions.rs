//! The origin's redacted objects: read at start and every few seconds,
//! dropped from disk and refused even to a valid grant.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use rill_types::ObjectHash;
use serde::Deserialize;

use crate::error::{Error, Result};

#[derive(Deserialize)]
struct Published {
    hashes: Vec<String>,
}

/// What the origin redacted.
pub struct Redactions {
    url: String,
    client: reqwest::Client,
    token: Option<String>,
    hashes: RwLock<HashSet<ObjectHash>>,
    read_at: RwLock<Option<Instant>>,
}

impl Redactions {
    /// The list `origin` publishes to holders of `token`.
    pub fn new(origin: &str, client: reqwest::Client, token: Option<String>) -> Self {
        Redactions {
            url: format!("{}/redactions", origin.trim_end_matches('/')),
            client,
            token,
            hashes: RwLock::new(HashSet::new()),
            read_at: RwLock::new(None),
        }
    }

    /// Whether the list was read within `max_age`.
    pub fn fresh(&self, max_age: Duration) -> bool {
        self.read_at.read().is_some_and(|t| t.elapsed() <= max_age)
    }

    /// Reread the list; returns the hashes newly redacted.
    pub async fn refresh(&self) -> Result<Vec<ObjectHash>> {
        let mut request = self.client.get(&self.url);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let published: Published = request
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| Error::Origin(format!("redactions: {e}")))?
            .json()
            .await
            .map_err(|e| Error::Origin(format!("redactions: {e}")))?;
        let hashes: HashSet<ObjectHash> = published
            .hashes
            .iter()
            .filter_map(|h| ObjectHash::from_hex(h).ok())
            .collect();
        let mut held = self.hashes.write();
        let new = hashes.difference(&held).copied().collect();
        *held = hashes;
        *self.read_at.write() = Some(Instant::now());
        Ok(new)
    }

    pub fn contains(&self, hash: &ObjectHash) -> bool {
        self.hashes.read().contains(hash)
    }
}
