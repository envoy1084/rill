//! Metadata store on [`redb`], an embedded, single-file, ACID B-tree.
//!
//! Everything lives in one table `kv` mapping raw key bytes to raw value
//! bytes; the ordered byte keys from [`crate::keys`] give prefix scans for
//! free. A [`Transaction`] maps onto one redb write transaction, so its
//! conditions and mutations are checked and applied against a single
//! consistent snapshot and commit atomically. redb serialises writers, which
//! is exactly the isolation the conditional-write protocol needs.

use std::collections::BTreeMap;
use std::ops::Bound;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use redb::{
    Database, ReadTransaction, ReadableDatabase, ReadableTable, TableDefinition, WriteTransaction,
};

use crate::error::{Error, Result};
use crate::memory::{check_conditions, next_seq};
use crate::meta::{seq_key, Condition, KeyRange, MetaStore, Mutation, Transaction, TxOutcome};

const KV: TableDefinition<&[u8], &[u8]> = TableDefinition::new("kv");

/// [`MetaStore`] backed by a redb database file.
#[derive(Debug, Clone)]
pub struct RedbMetaStore {
    db: Arc<Database>,
}

impl RedbMetaStore {
    /// Open or create the database at `path`.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        tokio::task::spawn_blocking(move || {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let db = Database::create(&path).map_err(Error::backend)?;
            Self::from_database(db)
        })
        .await
        .map_err(Error::backend)?
    }

    /// An in-memory redb database that is discarded on drop. Exercises the
    /// real transaction code paths without touching disk; meant for tests.
    pub fn temporary() -> Result<Self> {
        let db = redb::Builder::new()
            .create_with_backend(redb::backends::InMemoryBackend::new())
            .map_err(Error::backend)?;
        Self::from_database(db)
    }

    /// Wrap an already-open database.
    pub fn from_database(db: Database) -> Result<Self> {
        // Create the table up front so read transactions never have to
        // special-case a missing table.
        let tx = db.begin_write().map_err(Error::backend)?;
        tx.open_table(KV).map_err(Error::backend)?;
        tx.commit().map_err(Error::backend)?;
        Ok(RedbMetaStore { db: Arc::new(db) })
    }

    async fn with_read<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&ReadTransaction) -> Result<T> + Send + 'static,
    {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_read().map_err(Error::backend)?;
            f(&tx)
        })
        .await
        .map_err(Error::backend)?
    }
}

fn get_in(tx: &ReadTransaction, key: &[u8]) -> Result<Option<Bytes>> {
    let table = tx.open_table(KV).map_err(Error::backend)?;
    let value = table
        .get(key)
        .map_err(Error::backend)?
        .map(|guard| Bytes::copy_from_slice(guard.value()));
    Ok(value)
}

fn scan_in<T: ReadableTable<&'static [u8], &'static [u8]>>(
    table: &T,
    range: &KeyRange,
) -> Result<Vec<(Bytes, Bytes)>> {
    let bounds: (Bound<&[u8]>, Bound<&[u8]>) =
        (as_slice_bound(&range.start), as_slice_bound(&range.end));
    let iter = table.range::<&[u8]>(bounds).map_err(Error::backend)?;
    let iter: Box<dyn Iterator<Item = _>> = if range.reverse {
        Box::new(iter.rev())
    } else {
        Box::new(iter)
    };
    iter.take(range.limit)
        .map(|item| {
            let (k, v) = item.map_err(Error::backend)?;
            Ok((
                Bytes::copy_from_slice(k.value()),
                Bytes::copy_from_slice(v.value()),
            ))
        })
        .collect()
}

fn as_slice_bound(bound: &Bound<Bytes>) -> Bound<&[u8]> {
    match bound {
        Bound::Included(b) => Bound::Included(b.as_ref()),
        Bound::Excluded(b) => Bound::Excluded(b.as_ref()),
        Bound::Unbounded => Bound::Unbounded,
    }
}

/// Apply a transaction inside an open write transaction. Mirrors
/// `MemoryMetaStore::transact` step for step so the two backends agree.
fn apply(wtx: &WriteTransaction, tx: Transaction) -> Result<TxOutcome> {
    let mut table = wtx.open_table(KV).map_err(Error::backend)?;

    // Conditions are evaluated before any mutation, against the snapshot the
    // write transaction holds. Keys are read up front so a storage error is
    // reported as such rather than masquerading as a missing key.
    let mut current = BTreeMap::new();
    for cond in &tx.conditions {
        let key = match cond {
            Condition::Equals { key, .. }
            | Condition::Absent { key }
            | Condition::Present { key } => key,
        };
        let value = table
            .get(key.as_ref())
            .map_err(Error::backend)?
            .map(|g| Bytes::copy_from_slice(g.value()));
        current.insert(key.clone(), value);
    }
    check_conditions(&tx.conditions, |k| current.get(k).cloned().flatten())?;

    let mut outcome = TxOutcome::default();
    for m in tx.mutations {
        match m {
            Mutation::Set { key, value } => {
                table
                    .insert(key.as_ref(), value.as_ref())
                    .map_err(Error::backend)?;
            }
            Mutation::Delete { key } => {
                table.remove(key.as_ref()).map_err(Error::backend)?;
            }
            Mutation::AppendLog { prefix, value } => {
                let last = scan_in(&table, &KeyRange::prefix(prefix.clone()).reverse().limit(1))?
                    .pop()
                    .map(|(k, _)| k);
                let seq = next_seq(&prefix, last.as_deref());
                table
                    .insert(seq_key(&prefix, seq).as_ref(), value.as_ref())
                    .map_err(Error::backend)?;
                outcome.appended.push(seq);
            }
        }
    }
    Ok(outcome)
}

#[async_trait]
impl MetaStore for RedbMetaStore {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        let key = key.to_vec();
        self.with_read(move |tx| get_in(tx, &key)).await
    }

    async fn get_many(&self, keys: &[Bytes]) -> Result<Vec<Option<Bytes>>> {
        let keys = keys.to_vec();
        self.with_read(move |tx| {
            let table = tx.open_table(KV).map_err(Error::backend)?;
            keys.iter()
                .map(|k| {
                    Ok(table
                        .get(k.as_ref())
                        .map_err(Error::backend)?
                        .map(|g| Bytes::copy_from_slice(g.value())))
                })
                .collect()
        })
        .await
    }

    async fn scan(&self, range: KeyRange) -> Result<Vec<(Bytes, Bytes)>> {
        self.with_read(move |tx| {
            let table = tx.open_table(KV).map_err(Error::backend)?;
            scan_in(&table, &range)
        })
        .await
    }

    async fn transact(&self, tx: Transaction) -> Result<TxOutcome> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            let wtx = db.begin_write().map_err(Error::backend)?;
            match apply(&wtx, tx) {
                Ok(outcome) => {
                    wtx.commit().map_err(Error::backend)?;
                    Ok(outcome)
                }
                Err(e) => {
                    // Dropping an uncommitted write transaction aborts it;
                    // being explicit keeps the intent obvious.
                    wtx.abort().map_err(Error::backend)?;
                    Err(e)
                }
            }
        })
        .await
        .map_err(Error::backend)?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("meta.redb");
        {
            let store = RedbMetaStore::open(&path).await.unwrap();
            store
                .set(Bytes::from_static(b"k"), Bytes::from_static(b"v"))
                .await
                .unwrap();
        }
        let store = RedbMetaStore::open(&path).await.unwrap();
        assert_eq!(
            store.get(b"k").await.unwrap(),
            Some(Bytes::from_static(b"v"))
        );
    }

    #[tokio::test]
    async fn failed_condition_rolls_back_everything() {
        let store = RedbMetaStore::temporary().unwrap();
        let err = store
            .transact(
                Transaction::new()
                    .set(&b"a"[..], &b"1"[..])
                    .expect_present(&b"missing"[..]),
            )
            .await
            .unwrap_err();
        assert!(err.is_conflict());
        assert_eq!(store.get(b"a").await.unwrap(), None);
    }

    #[tokio::test]
    async fn get_many_matches_get() {
        let store = RedbMetaStore::temporary().unwrap();
        store
            .transact(Transaction::new().set(&b"x"[..], &b"1"[..]))
            .await
            .unwrap();
        let got = store
            .get_many(&[Bytes::from_static(b"x"), Bytes::from_static(b"y")])
            .await
            .unwrap();
        assert_eq!(got, vec![Some(Bytes::from_static(b"1")), None]);
    }
}
