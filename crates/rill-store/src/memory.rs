//! In-memory backends, used by tests, the single-process server mode and as
//! the reference semantics every other backend must match.

use std::collections::{BTreeMap, HashMap};
use std::ops::Bound;

use async_trait::async_trait;
use bytes::Bytes;
use parking_lot::RwLock;
use rill_types::{ObjectHash, ObjectKind};

use crate::error::{Error, Result};
use crate::meta::{Condition, KeyRange, MetaStore, Mutation, Transaction, TxOutcome};
use crate::object::{verify, ObjectStore};

#[derive(Default)]
pub struct MemoryObjectStore {
    objects: RwLock<HashMap<(ObjectKind, ObjectHash), Bytes>>,
}

impl MemoryObjectStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.objects.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.objects.read().is_empty()
    }
}

#[async_trait]
impl ObjectStore for MemoryObjectStore {
    async fn get(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<Option<Bytes>> {
        Ok(self.objects.read().get(&(kind, *hash)).cloned())
    }

    async fn has(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<bool> {
        Ok(self.objects.read().contains_key(&(kind, *hash)))
    }

    async fn put(&self, kind: ObjectKind, hash: &ObjectHash, bytes: Bytes) -> Result<()> {
        verify(kind, hash, &bytes)?;
        self.objects.write().entry((kind, *hash)).or_insert(bytes);
        Ok(())
    }
}

#[derive(Default)]
pub struct MemoryMetaStore {
    data: RwLock<BTreeMap<Bytes, Bytes>>,
}

impl MemoryMetaStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.data.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.read().is_empty()
    }
}

/// Check conditions against a snapshot; shared by backends that evaluate
/// transactions in process.
pub fn check_conditions(
    conditions: &[Condition],
    lookup: impl Fn(&[u8]) -> Option<Bytes>,
) -> Result<()> {
    for cond in conditions {
        match cond {
            Condition::Equals { key, value } => {
                let current = lookup(key);
                if &current != value {
                    return Err(Error::Conflict(format!(
                        "key {} changed",
                        String::from_utf8_lossy(key)
                    )));
                }
            }
            Condition::Absent { key } => {
                if lookup(key).is_some() {
                    return Err(Error::Conflict(format!(
                        "key {} exists",
                        String::from_utf8_lossy(key)
                    )));
                }
            }
            Condition::Present { key } => {
                if lookup(key).is_none() {
                    return Err(Error::Conflict(format!(
                        "key {} missing",
                        String::from_utf8_lossy(key)
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Next sequence number for an append-log prefix given the highest existing
/// key under that prefix.
pub fn next_seq(prefix: &[u8], last_key: Option<&[u8]>) -> u64 {
    match last_key.and_then(|k| crate::meta::seq_from_key(prefix.len(), k)) {
        Some(last) => last + 1,
        None => 1,
    }
}

#[async_trait]
impl MetaStore for MemoryMetaStore {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        Ok(self.data.read().get(key).cloned())
    }

    async fn scan(&self, range: KeyRange) -> Result<Vec<(Bytes, Bytes)>> {
        let data = self.data.read();
        let bounds = (
            match &range.start {
                Bound::Included(s) => Bound::Included(s.clone()),
                Bound::Excluded(s) => Bound::Excluded(s.clone()),
                Bound::Unbounded => Bound::Unbounded,
            },
            match &range.end {
                Bound::Included(e) => Bound::Included(e.clone()),
                Bound::Excluded(e) => Bound::Excluded(e.clone()),
                Bound::Unbounded => Bound::Unbounded,
            },
        );
        let iter = data.range::<Bytes, _>(bounds);
        let out: Vec<(Bytes, Bytes)> = if range.reverse {
            iter.rev()
                .take(range.limit)
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        } else {
            iter.take(range.limit)
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };
        Ok(out)
    }

    async fn transact(&self, tx: Transaction) -> Result<TxOutcome> {
        let mut data = self.data.write();
        check_conditions(&tx.conditions, |k| data.get(k).cloned())?;
        let mut outcome = TxOutcome::default();
        for m in tx.mutations {
            match m {
                Mutation::Set { key, value } => {
                    data.insert(key, value);
                }
                Mutation::Delete { key } => {
                    data.remove(&key);
                }
                Mutation::AppendLog { prefix, value } => {
                    let last = {
                        let range = KeyRange::prefix(prefix.clone());
                        let (start, end) = (range.start, range.end);
                        data.range::<Bytes, _>((start, end))
                            .next_back()
                            .map(|(k, _)| k.clone())
                    };
                    let seq = next_seq(&prefix, last.as_deref());
                    data.insert(crate::meta::seq_key(&prefix, seq), value);
                    outcome.appended.push(seq);
                }
            }
        }
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[tokio::test]
    async fn transactions_are_conditional() {
        let store = MemoryMetaStore::new();
        store
            .transact(Transaction::new().set(&b"a"[..], &b"1"[..]))
            .await
            .unwrap();
        let err = store
            .transact(
                Transaction::new()
                    .expect(&b"a"[..], Some(Bytes::from_static(b"2")))
                    .set(&b"a"[..], &b"3"[..]),
            )
            .await
            .unwrap_err();
        assert!(err.is_conflict());
        store
            .transact(
                Transaction::new()
                    .expect(&b"a"[..], Some(Bytes::from_static(b"1")))
                    .set(&b"a"[..], &b"3"[..]),
            )
            .await
            .unwrap();
        assert_eq!(
            store.get(b"a").await.unwrap(),
            Some(Bytes::from_static(b"3"))
        );
    }

    #[tokio::test]
    async fn append_log_assigns_sequences() {
        let store = MemoryMetaStore::new();
        let out = store
            .transact(
                Transaction::new()
                    .append_log(&b"log/"[..], &b"x"[..])
                    .append_log(&b"log/"[..], &b"y"[..]),
            )
            .await
            .unwrap();
        assert_eq!(out.appended, vec![1, 2]);
        let rows = store.scan(KeyRange::prefix(&b"log/"[..])).await.unwrap();
        assert_eq!(rows.len(), 2);
        let rev = store
            .scan(KeyRange::prefix(&b"log/"[..]).reverse().limit(1))
            .await
            .unwrap();
        assert_eq!(rev[0].1, Bytes::from_static(b"y"));
    }
}
