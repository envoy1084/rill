//! In-memory backends, used by tests, the single-process server mode and as
//! the reference semantics every other backend must match.

use std::collections::{BTreeMap, HashMap};
use std::ops::Bound;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use parking_lot::RwLock;
use rill_types::{ObjectHash, ObjectKind};

use crate::error::Result;
use crate::keyspace::buffered::{resolve_stamps, stamp_of};
use crate::keyspace::engine::add_le;
use crate::keyspace::{Batch, Buffered, Keyspace, Op, RawRange};
use crate::object::{verify, ObjectStore};

type ObjectMap = HashMap<(Arc<str>, ObjectKind, ObjectHash), Bytes>;

/// Objects of every namespace in one map; each handle sees its own.
pub struct MemoryObjectStore {
    objects: Arc<RwLock<ObjectMap>>,
    namespace: Arc<str>,
}

impl Default for MemoryObjectStore {
    fn default() -> Self {
        MemoryObjectStore {
            objects: Arc::default(),
            namespace: Arc::from(crate::seal::DEFAULT_NAMESPACE),
        }
    }
}

impl MemoryObjectStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Objects in this handle's namespace.
    pub fn len(&self) -> usize {
        self.objects
            .read()
            .keys()
            .filter(|(ns, _, _)| *ns == self.namespace)
            .count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn key(&self, kind: ObjectKind, hash: &ObjectHash) -> (Arc<str>, ObjectKind, ObjectHash) {
        (self.namespace.clone(), kind, *hash)
    }
}

#[async_trait]
impl ObjectStore for MemoryObjectStore {
    async fn get(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<Option<Bytes>> {
        Ok(self.objects.read().get(&self.key(kind, hash)).cloned())
    }

    async fn has(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<bool> {
        Ok(self.objects.read().contains_key(&self.key(kind, hash)))
    }

    async fn put(&self, kind: ObjectKind, hash: &ObjectHash, bytes: Bytes) -> Result<()> {
        verify(kind, hash, &bytes)?;
        self.objects
            .write()
            .entry(self.key(kind, hash))
            .or_insert(bytes);
        Ok(())
    }

    fn namespaced(&self, namespace: &str) -> Arc<dyn ObjectStore> {
        Arc::new(MemoryObjectStore {
            objects: self.objects.clone(),
            namespace: Arc::from(namespace),
        })
    }
}

/// The in-memory engine: a sorted map behind a lock.
#[derive(Default)]
pub struct MemoryBatch {
    data: RwLock<BTreeMap<Bytes, Bytes>>,
    commits: AtomicU64,
}

/// The in-memory metadata store.
pub type MemoryMetaStore = Keyspace<Buffered<MemoryBatch>>;

impl Keyspace<Buffered<MemoryBatch>> {
    /// An empty store.
    pub fn new() -> Self {
        Keyspace::with_engine(Buffered::new(MemoryBatch::default()))
    }
}

impl Default for Keyspace<Buffered<MemoryBatch>> {
    fn default() -> Self {
        Self::new()
    }
}

/// The rows of `data` in `range`.
pub fn scan_map(data: &BTreeMap<Bytes, Bytes>, range: &RawRange) -> Vec<(Bytes, Bytes)> {
    let iter = data.range::<[u8], _>((
        Bound::Included(range.start.as_ref()),
        Bound::Excluded(range.end.as_ref()),
    ));
    let pair = |(k, v): (&Bytes, &Bytes)| (k.clone(), v.clone());
    if range.reverse {
        iter.rev().take(range.limit).map(pair).collect()
    } else {
        iter.take(range.limit).map(pair).collect()
    }
}

#[async_trait]
impl Batch for MemoryBatch {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        Ok(self.data.read().get(key).cloned())
    }

    async fn scan(&self, range: &RawRange) -> Result<Vec<(Bytes, Bytes)>> {
        if range.start >= range.end {
            return Ok(Vec::new());
        }
        Ok(scan_map(&self.data.read(), range))
    }

    async fn apply(&self, ops: Vec<Op>, stamp: bool) -> Result<Option<[u8; 10]>> {
        let mut data = self.data.write();
        let stamp = stamp.then(|| stamp_of(self.commits.load(Ordering::SeqCst) + 1));
        let ops = resolve_stamps(ops, stamp.as_ref())?;
        if stamp.is_some() {
            self.commits.fetch_add(1, Ordering::SeqCst);
        }
        for op in ops {
            match op {
                Op::Set(k, v) | Op::SetStamped(k, v) => {
                    data.insert(k, v);
                }
                Op::Clear(k) => {
                    data.remove(&k);
                }
                Op::ClearRange(start, end) => {
                    let doomed: Vec<Bytes> = scan_map(
                        &data,
                        &RawRange {
                            start,
                            end,
                            limit: usize::MAX,
                            reverse: false,
                        },
                    )
                    .into_iter()
                    .map(|(k, _)| k)
                    .collect();
                    for k in doomed {
                        data.remove(&k);
                    }
                }
                Op::Add(k, delta) => {
                    let sum = add_le(data.get(&k).map(|v| v.as_ref()), delta);
                    data.insert(k, Bytes::copy_from_slice(&sum));
                }
            }
        }
        Ok(stamp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;

    #[tokio::test]
    async fn object_store_verifies_hash() {
        let store = MemoryObjectStore::new();
        let bytes = Bytes::from_static(b"hello");
        let good = ObjectHash::digest(ObjectKind::Blob, &bytes);
        store
            .put(ObjectKind::Blob, &good, bytes.clone())
            .await
            .unwrap();
        assert_eq!(
            store.get(ObjectKind::Blob, &good).await.unwrap(),
            Some(bytes.clone())
        );
        let bad = ObjectHash::digest(ObjectKind::Blob, b"other");
        assert!(matches!(
            store.put(ObjectKind::Blob, &bad, bytes).await,
            Err(Error::HashMismatch { .. })
        ));
    }
}
