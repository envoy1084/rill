//! The cache itself: disk, grant keys, redactions and coalesced origin
//! fetches behind one object lookup.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use bytes::Bytes;
use rill_policy::grants::ReadGrant;
use rill_types::{BlobManifest, Encodable, ObjectHash, ObjectKind};

use crate::coalesce::Coalescer;
use crate::config::CacheConfig;
use crate::disk::DiskCache;
use crate::error::{Error, Result};
use crate::keys::OriginKeys;
use crate::origin::{Credentials, Origin};

/// Counters a cache keeps.
#[derive(Debug, Default)]
pub struct CacheStats {
    pub hits: AtomicU64,
    pub misses: AtomicU64,
    /// Misses served by another request's origin fetch.
    pub coalesced: AtomicU64,
    pub origin_fetches: AtomicU64,
    pub denied: AtomicU64,
}

/// A running cache.
pub struct Cache {
    pub(super) disk: Arc<DiskCache>,
    pub(super) keys: Arc<OriginKeys>,
    pub(super) redactions: Arc<crate::redactions::Redactions>,
    pub(super) redaction_max_age: std::time::Duration,
    pub(super) origin: Origin,
    pub(super) inflight: Coalescer<(ObjectKind, ObjectHash), Bytes>,
    pub(super) stats: CacheStats,
}

impl Cache {
    /// Open the disk store and read the origin's grant keys.
    pub async fn start(config: &CacheConfig) -> Result<Arc<Self>> {
        let client = reqwest::Client::new();
        let disk = DiskCache::open(&config.dir, config.max_bytes).await?;
        let keys = OriginKeys::fetch(&config.origin, client.clone()).await?;
        let redactions = Arc::new(crate::redactions::Redactions::new(
            &config.origin,
            client.clone(),
            config.origin_token.clone(),
        ));
        for hash in redactions.refresh().await? {
            disk.remove(&hash).await?;
        }
        Ok(Arc::new(Cache {
            disk,
            keys,
            redactions,
            redaction_max_age: config.redaction_max_age,
            origin: Origin::new(&config.origin, client),
            inflight: Coalescer::new(),
            stats: CacheStats::default(),
        }))
    }

    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    pub fn disk(&self) -> &DiskCache {
        &self.disk
    }

    /// The object, from disk or the origin, once `grant` is seen to name
    /// it.
    pub(super) async fn object(
        &self,
        grant: &ReadGrant,
        grant_text: &str,
        authorization: Option<&str>,
        kind: ObjectKind,
        hash: &ObjectHash,
        via: Option<&ObjectHash>,
    ) -> Result<Bytes> {
        if !self.redactions.fresh(self.redaction_max_age) {
            return Err(Error::Unavailable(
                "the origin's redactions could not be reread; serving nothing until they are"
                    .into(),
            ));
        }
        if self.redactions.contains(hash) {
            return Err(Error::Redacted(format!("{kind} {hash}")));
        }
        match kind {
            ObjectKind::Chunk => {
                let via = via.ok_or_else(|| {
                    Error::Denied("a chunk is fetched through its manifest (`via`)".into())
                })?;
                let manifest = Box::pin(self.object(
                    grant,
                    grant_text,
                    authorization,
                    ObjectKind::BlobManifest,
                    via,
                    None,
                ))
                .await?;
                let listed = BlobManifest::decode(&manifest)
                    .map_err(|e| Error::Origin(format!("manifest {via}: {e}")))?;
                if !listed.chunks.iter().any(|c| c.hash == *hash) {
                    return Err(Error::Denied(format!(
                        "manifest {via} does not list chunk {hash}"
                    )));
                }
            }
            ObjectKind::Derived => {
                return Err(Error::Denied("derived values are not served here".into()))
            }
            _ => {
                if grant.entry(kind, hash).is_none() {
                    return Err(Error::Denied(format!(
                        "the read grant does not name {kind} {hash}"
                    )));
                }
            }
        }
        if let Some(bytes) = self.disk.get(kind, hash).await? {
            self.stats.hits.fetch_add(1, Ordering::Relaxed);
            return Ok(bytes);
        }
        self.stats.misses.fetch_add(1, Ordering::Relaxed);
        let creds = Credentials {
            authorization,
            grant: grant_text,
            via,
        };
        let (result, shared) = self
            .inflight
            .get((kind, *hash), || self.fill(grant, kind, hash, &creds))
            .await;
        if shared {
            self.stats.coalesced.fetch_add(1, Ordering::Relaxed);
        }
        result
    }

    async fn fill(
        &self,
        grant: &ReadGrant,
        kind: ObjectKind,
        hash: &ObjectHash,
        creds: &Credentials<'_>,
    ) -> Result<Bytes> {
        self.stats.origin_fetches.fetch_add(1, Ordering::Relaxed);
        let bytes = self
            .origin
            .fetch(grant.repo, kind, hash, creds)
            .await?
            .ok_or_else(|| Error::NotFound(format!("{kind} {hash}")))?;
        rill_store::object::verify(kind, hash, &bytes)
            .map_err(|e| Error::Corrupt(e.to_string()))?;
        if let Err(e) = self.disk.put(kind, hash, bytes.clone()).await {
            tracing::warn!(error = %e, %kind, %hash, "keeping an object on disk failed");
        }
        Ok(bytes)
    }
}
