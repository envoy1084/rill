//! Reading blobs through a regional cache (`rill-cache`): `GET
//! /o/{repo}/blob/{hash}` with the read grant a listing returned. Any
//! failure falls back to the origin, so the cache only ever saves trips.

use bytes::Bytes;
use rill_types::{ObjectHash, ObjectKind};

/// A regional cache in front of the origin.
#[derive(Debug, Clone)]
pub(crate) struct CacheTier {
    base: String,
    http: reqwest::Client,
}

impl CacheTier {
    /// The cache at `base` (`http://host:port`).
    pub(crate) fn new(base: &str) -> Self {
        CacheTier {
            base: base.trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }

    /// The blob `hash` of `repo`, checked against its hash; `None` when the
    /// cache cannot serve it.
    pub(crate) async fn blob(
        &self,
        repo: &str,
        hash: &ObjectHash,
        grant: &str,
        bearer: Option<String>,
    ) -> Option<Bytes> {
        let segment = repo.rsplit('/').next().unwrap_or(repo);
        let url = format!("{}/o/{segment}/blob/{}", self.base, hash.to_hex());
        let mut request = self.http.get(&url).header("x-rill-grant", grant);
        if let Some(token) = bearer {
            request = request.bearer_auth(token);
        }
        let response = match request.send().await {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                tracing::debug!(%url, status = %r.status(), "cache refused; reading from the origin");
                return None;
            }
            Err(e) => {
                tracing::debug!(%url, error = %e, "cache unreachable; reading from the origin");
                return None;
            }
        };
        let bytes = response.bytes().await.ok()?;
        (ObjectHash::digest(ObjectKind::Blob, &bytes) == *hash).then_some(bytes)
    }
}
