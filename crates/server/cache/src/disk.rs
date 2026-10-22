//! Objects on local disk, least recently used evicted past a byte budget.
//!
//! Layout: `<dir>/<kind>/<hh>/<hash-hex>`, the raw object bytes. The index
//! lives in memory and is rebuilt from the directory at start. Every read
//! is hash-checked, so a damaged file is dropped and refetched rather than
//! served.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use bytes::Bytes;
use parking_lot::Mutex;
use rill_types::{ObjectHash, ObjectKind};

use crate::error::{Error, Result};

type Key = (ObjectKind, ObjectHash);

#[derive(Default)]
struct Index {
    entries: HashMap<Key, (u64, u64)>,
    by_use: BTreeMap<u64, Key>,
    bytes: u64,
    tick: u64,
}

impl Index {
    fn touch(&mut self, key: Key, size: u64) {
        if let Some((old_tick, old_size)) = self.entries.remove(&key) {
            self.by_use.remove(&old_tick);
            self.bytes -= old_size;
        }
        self.tick += 1;
        self.entries.insert(key, (self.tick, size));
        self.by_use.insert(self.tick, key);
        self.bytes += size;
    }

    fn remove(&mut self, key: &Key) {
        if let Some((tick, size)) = self.entries.remove(key) {
            self.by_use.remove(&tick);
            self.bytes -= size;
        }
    }

    fn oldest(&self) -> Option<Key> {
        self.by_use.values().next().copied()
    }
}

/// The disk store.
pub struct DiskCache {
    root: PathBuf,
    max_bytes: u64,
    index: Mutex<Index>,
    temp: AtomicU64,
    evictions: AtomicU64,
}

fn path_for(root: &Path, kind: ObjectKind, hash: &ObjectHash) -> PathBuf {
    let hex = hash.to_hex();
    root.join(kind.as_str()).join(&hex[..2]).join(hex)
}

fn verified(kind: ObjectKind, hash: &ObjectHash, bytes: &[u8]) -> bool {
    rill_store::object::verify(kind, hash, bytes).is_ok()
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| Error::Io(std::io::Error::other(e)))?
}

impl DiskCache {
    /// Open (creating) the store in `root`, indexing what is already there.
    pub async fn open(root: impl Into<PathBuf>, max_bytes: u64) -> Result<Arc<Self>> {
        let root = root.into();
        let scan_root = root.clone();
        let found = blocking(move || {
            fs::create_dir_all(&scan_root)?;
            let mut found = Vec::new();
            for kind in [
                ObjectKind::Blob,
                ObjectKind::Chunk,
                ObjectKind::BlobManifest,
                ObjectKind::Tree,
                ObjectKind::ChangeVersion,
            ] {
                let dir = scan_root.join(kind.as_str());
                let Ok(shards) = fs::read_dir(&dir) else {
                    continue;
                };
                for shard in shards.flatten() {
                    let Ok(files) = fs::read_dir(shard.path()) else {
                        continue;
                    };
                    for file in files.flatten() {
                        let name = file.file_name();
                        let Some(hash) = name.to_str().and_then(|n| ObjectHash::from_hex(n).ok())
                        else {
                            // A temp file a crash left behind.
                            let _ = fs::remove_file(file.path());
                            continue;
                        };
                        let size = file.metadata().map(|m| m.len()).unwrap_or(0);
                        let used = file.metadata().and_then(|m| m.accessed()).ok();
                        found.push((used, (kind, hash), size));
                    }
                }
            }
            found.sort_by_key(|(used, _, _)| *used);
            Ok(found)
        })
        .await?;
        let cache = Arc::new(DiskCache {
            root,
            max_bytes,
            index: Mutex::new(Index::default()),
            temp: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
        });
        {
            let mut index = cache.index.lock();
            for (_, key, size) in found {
                index.touch(key, size);
            }
        }
        cache.evict().await?;
        Ok(cache)
    }

    /// Bytes held.
    pub fn bytes(&self) -> u64 {
        self.index.lock().bytes
    }

    /// Objects held.
    pub fn len(&self) -> usize {
        self.index.lock().entries.len()
    }

    /// Whether nothing is held.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Objects evicted since start.
    pub fn evictions(&self) -> u64 {
        self.evictions.load(Ordering::Relaxed)
    }

    /// The object, when held and intact.
    pub async fn get(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<Option<Bytes>> {
        let key = (kind, *hash);
        let Some(size) = self.index.lock().entries.get(&key).map(|(_, s)| *s) else {
            return Ok(None);
        };
        let path = path_for(&self.root, kind, hash);
        let hash_copy = *hash;
        let read = blocking(move || match fs::read(&path) {
            Ok(bytes) if verified(kind, &hash_copy, &bytes) => Ok(Some(Bytes::from(bytes))),
            Ok(_) => {
                tracing::warn!(%kind, hash = %hash_copy, "cached object failed its hash check; dropping it");
                let _ = fs::remove_file(&path);
                Ok(None)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        })
        .await?;
        let mut index = self.index.lock();
        match &read {
            Some(_) => index.touch(key, size),
            None => index.remove(&key),
        }
        Ok(read)
    }

    /// Drop every kind of object with `hash`.
    pub async fn remove(&self, hash: &ObjectHash) -> Result<()> {
        let held: Vec<Key> = {
            let index = self.index.lock();
            index
                .entries
                .keys()
                .filter(|(_, h)| h == hash)
                .copied()
                .collect()
        };
        for key in held {
            self.index.lock().remove(&key);
            let path = path_for(&self.root, key.0, &key.1);
            blocking(move || match fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
                _ => Ok(()),
            })
            .await?;
        }
        Ok(())
    }

    /// Keep `bytes` as `(kind, hash)`, already verified by the caller, then
    /// evict past the budget. An object larger than the whole budget is not
    /// kept.
    pub async fn put(&self, kind: ObjectKind, hash: &ObjectHash, bytes: Bytes) -> Result<()> {
        let size = bytes.len() as u64;
        if size > self.max_bytes {
            return Ok(());
        }
        let path = path_for(&self.root, kind, hash);
        let temp = self.temp.fetch_add(1, Ordering::Relaxed);
        blocking(move || {
            let dir = path
                .parent()
                .ok_or_else(|| Error::Config(format!("{} has no parent", path.display())))?;
            fs::create_dir_all(dir)?;
            let tmp = dir.join(format!(".tmp-{}-{temp}", std::process::id()));
            let mut file = fs::File::create(&tmp)?;
            file.write_all(&bytes)?;
            fs::rename(&tmp, &path)?;
            Ok(())
        })
        .await?;
        self.index.lock().touch((kind, *hash), size);
        self.evict().await
    }

    async fn evict(&self) -> Result<()> {
        loop {
            let victim = {
                let mut index = self.index.lock();
                if index.bytes <= self.max_bytes {
                    return Ok(());
                }
                let Some(key) = index.oldest() else {
                    return Ok(());
                };
                index.remove(&key);
                key
            };
            self.evictions.fetch_add(1, Ordering::Relaxed);
            let path = path_for(&self.root, victim.0, &victim.1);
            blocking(move || match fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
                _ => Ok(()),
            })
            .await?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(n: u8) -> (ObjectHash, Bytes) {
        let bytes = Bytes::from(vec![n; 100]);
        (ObjectHash::digest(ObjectKind::Blob, &bytes), bytes)
    }

    #[tokio::test]
    async fn the_least_recently_used_go_first_and_the_index_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let cache = DiskCache::open(dir.path(), 250).await.unwrap();
        let (a, a_bytes) = object(1);
        let (b, b_bytes) = object(2);
        let (c, c_bytes) = object(3);
        cache
            .put(ObjectKind::Blob, &a, a_bytes.clone())
            .await
            .unwrap();
        cache.put(ObjectKind::Blob, &b, b_bytes).await.unwrap();
        // Reading `a` makes `b` the oldest.
        assert_eq!(
            cache.get(ObjectKind::Blob, &a).await.unwrap(),
            Some(a_bytes.clone())
        );
        cache.put(ObjectKind::Blob, &c, c_bytes).await.unwrap();
        assert_eq!(cache.len(), 2);
        assert!(cache.bytes() <= 250);
        assert_eq!(cache.get(ObjectKind::Blob, &b).await.unwrap(), None);
        assert_eq!(cache.evictions(), 1);
        drop(cache);

        let reopened = DiskCache::open(dir.path(), 250).await.unwrap();
        assert_eq!(reopened.len(), 2);
        assert_eq!(
            reopened.get(ObjectKind::Blob, &a).await.unwrap(),
            Some(a_bytes)
        );
        assert!(
            DiskCache::open(dir.path(), 100).await.unwrap().len() <= 1,
            "a smaller budget evicts at start"
        );
    }

    #[tokio::test]
    async fn a_damaged_file_is_a_miss_not_a_wrong_answer() {
        let dir = tempfile::tempdir().unwrap();
        let cache = DiskCache::open(dir.path(), 1_000).await.unwrap();
        let (a, a_bytes) = object(7);
        cache.put(ObjectKind::Blob, &a, a_bytes).await.unwrap();
        std::fs::write(path_for(dir.path(), ObjectKind::Blob, &a), b"flipped").unwrap();
        assert_eq!(cache.get(ObjectKind::Blob, &a).await.unwrap(), None);
        assert!(cache.is_empty());
    }
}
