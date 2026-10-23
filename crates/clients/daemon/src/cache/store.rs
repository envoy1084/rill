//! The shared, content-addressed object cache on disk: one file per
//! object under `objects/ab/<hash>`, written atomically, evicted by
//! [`Lru`] within a byte budget. Blocking; call it off the async runtime.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use bytes::Bytes;
use parking_lot::Mutex;
use rill_types::ObjectHash;

use super::lru::Lru;
use crate::error::Result;

/// Counters reported by `GetDaemon`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Objects held.
    pub objects: u64,
    /// Bytes held.
    pub bytes: u64,
    /// The byte budget.
    pub budget: u64,
    /// Reads served from the cache.
    pub hits: u64,
    /// Reads that had to fetch.
    pub misses: u64,
    /// Objects evicted.
    pub evictions: u64,
}

/// The on-disk object cache.
#[derive(Debug)]
pub struct ObjectCache {
    dir: PathBuf,
    lru: Mutex<Lru>,
    hits: AtomicU64,
    misses: AtomicU64,
    evictions: AtomicU64,
}

impl ObjectCache {
    /// Open (or create) the cache in `dir`, re-indexing what is there by
    /// modification time, oldest first.
    pub fn open(dir: impl Into<PathBuf>, budget: u64) -> Result<Self> {
        let dir = dir.into();
        let objects = dir.join("objects");
        fs::create_dir_all(&objects)?;
        let mut found = Vec::new();
        for shard in fs::read_dir(&objects)? {
            let shard = shard?;
            if !shard.file_type()?.is_dir() {
                continue;
            }
            for entry in fs::read_dir(shard.path())? {
                let entry = entry?;
                let name = entry.file_name();
                let Some(hash) = name.to_str().and_then(|n| ObjectHash::from_hex(n).ok()) else {
                    // A temporary file from an interrupted write.
                    let _ = fs::remove_file(entry.path());
                    continue;
                };
                let md = entry.metadata()?;
                let at = md.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                found.push((at, hash, md.len()));
            }
        }
        found.sort();
        let cache = ObjectCache {
            dir,
            lru: Mutex::new(Lru::new(budget)),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
        };
        for (_, hash, size) in found {
            let evicted = cache.lru.lock().insert(hash, size);
            cache.delete(&evicted);
        }
        Ok(cache)
    }

    /// Where `hash` is (or would be) stored.
    pub fn path_of(&self, hash: &ObjectHash) -> PathBuf {
        let hex = hash.to_hex();
        self.dir.join("objects").join(&hex[..2]).join(hex)
    }

    /// Whether `hash` is held, without counting a read.
    pub fn contains(&self, hash: &ObjectHash) -> bool {
        self.lru.lock().contains(hash)
    }

    /// The object's bytes, counting a hit or a miss.
    pub fn get(&self, hash: &ObjectHash) -> Result<Option<Bytes>> {
        if !self.lru.lock().touch(hash) {
            self.misses.fetch_add(1, Ordering::Relaxed);
            return Ok(None);
        }
        match fs::read(self.path_of(hash)) {
            Ok(bytes) => {
                self.hits.fetch_add(1, Ordering::Relaxed);
                Ok(Some(Bytes::from(bytes)))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Removed behind the daemon's back.
                self.lru.lock().remove(hash);
                self.misses.fetch_add(1, Ordering::Relaxed);
                Ok(None)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Store an object (a no-op when held) and evict to the budget.
    pub fn put(&self, hash: &ObjectHash, bytes: &[u8]) -> Result<()> {
        if self.lru.lock().touch(hash) {
            return Ok(());
        }
        let path = self.path_of(hash);
        write_atomically(&path, bytes)?;
        let evicted = self.lru.lock().insert(*hash, bytes.len() as u64);
        self.delete(&evicted);
        Ok(())
    }

    /// Keep `hash` until a matching [`ObjectCache::unpin`].
    pub fn pin(&self, hash: &ObjectHash) -> bool {
        self.lru.lock().pin(hash)
    }

    /// Release one pin.
    pub fn unpin(&self, hash: &ObjectHash) {
        let evicted = self.lru.lock().unpin(hash);
        self.delete(&evicted);
    }

    /// Current counters.
    pub fn stats(&self) -> CacheStats {
        let lru = self.lru.lock();
        CacheStats {
            objects: lru.len() as u64,
            bytes: lru.bytes(),
            budget: lru.budget(),
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            evictions: self.evictions.load(Ordering::Relaxed),
        }
    }

    fn delete(&self, evicted: &[ObjectHash]) {
        for hash in evicted {
            self.evictions.fetch_add(1, Ordering::Relaxed);
            if let Err(e) = fs::remove_file(self.path_of(hash)) {
                tracing::debug!(%hash, error = %e, "evicted object already gone");
            }
        }
    }
}

/// Write to a temporary sibling, then rename, so a reader never sees a
/// partial object.
pub(crate) fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut tmp = tempfile_in(parent)?;
    tmp.1.write_all(bytes)?;
    tmp.1.sync_data()?;
    fs::rename(&tmp.0, path)?;
    Ok(())
}

fn tempfile_in(dir: &Path) -> Result<(PathBuf, fs::File)> {
    let name = format!(".rill-tmp-{}", uuid::Uuid::new_v4().simple());
    let path = dir.join(name);
    Ok((path.clone(), fs::File::create(&path)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_types::ObjectKind;

    #[test]
    fn reopening_keeps_objects_and_evicts_over_budget() {
        let dir = tempfile::tempdir().unwrap();
        let a = ObjectHash::digest(ObjectKind::Blob, b"a");
        let b = ObjectHash::digest(ObjectKind::Blob, b"b");
        {
            let cache = ObjectCache::open(dir.path(), 100).unwrap();
            cache.put(&a, &[0; 60]).unwrap();
            cache.put(&b, &[1; 30]).unwrap();
        }
        let cache = ObjectCache::open(dir.path(), 100).unwrap();
        assert_eq!(cache.get(&b).unwrap().unwrap().len(), 30);
        let c = ObjectHash::digest(ObjectKind::Blob, b"c");
        cache.put(&c, &[2; 30]).unwrap();
        assert!(!cache.contains(&a), "the oldest object is evicted");
        assert!(!cache.path_of(&a).exists());
        assert_eq!(cache.stats().evictions, 1);
    }
}
