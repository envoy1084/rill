//! The origin's redacted objects: read at start and every few seconds,
//! dropped from disk and refused even to a valid grant.

use std::collections::HashSet;

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
    hashes: RwLock<HashSet<ObjectHash>>,
}

impl Redactions {
    pub fn new(origin: &str, client: reqwest::Client) -> Self {
        Redactions {
            url: format!("{}/redactions", origin.trim_end_matches('/')),
            client,
            hashes: RwLock::new(HashSet::new()),
        }
    }

    /// Reread the list; returns the hashes newly redacted.
    pub async fn refresh(&self) -> Result<Vec<ObjectHash>> {
        let published: Published = self
            .client
            .get(&self.url)
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
        Ok(new)
    }

    pub fn contains(&self, hash: &ObjectHash) -> bool {
        self.hashes.read().contains(hash)
    }
}
