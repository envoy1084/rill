//! Metadata store on [`redb`], an embedded, single-file, ACID B-tree.
//!
//! Engine rows live in table `kv`; table `sys` holds the commit counter
//! that stamps are drawn from, so stamps keep increasing across restarts.
//! A batch is one redb write transaction.

use std::ops::Bound;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};

use crate::error::{Error, Result};
use crate::keyspace::buffered::{resolve_stamps, stamp_of};
use crate::keyspace::engine::add_le;
use crate::keyspace::{Batch, Buffered, Keyspace, Op, RawRange};

const KV: TableDefinition<&[u8], &[u8]> = TableDefinition::new("kv");
const SYS: TableDefinition<&str, u64> = TableDefinition::new("sys");
const COMMITS: &str = "commits";

/// The redb engine.
#[derive(Debug, Clone)]
pub struct RedbBatch {
    db: Arc<Database>,
}

/// The redb metadata store.
pub type RedbMetaStore = Keyspace<Buffered<RedbBatch>>;

impl Keyspace<Buffered<RedbBatch>> {
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
        // Create the tables up front so read transactions never have to
        // special-case a missing table.
        let tx = db.begin_write().map_err(Error::backend)?;
        tx.open_table(KV).map_err(Error::backend)?;
        tx.open_table(SYS).map_err(Error::backend)?;
        tx.commit().map_err(Error::backend)?;
        Ok(Keyspace::with_engine(Buffered::new(RedbBatch {
            db: Arc::new(db),
        })))
    }
}

impl RedbBatch {
    async fn blocking<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Database) -> Result<T> + Send + 'static,
    {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || f(&db))
            .await
            .map_err(Error::backend)?
    }
}

fn scan_table<T: ReadableTable<&'static [u8], &'static [u8]>>(
    table: &T,
    range: &RawRange,
) -> Result<Vec<(Bytes, Bytes)>> {
    let bounds: (Bound<&[u8]>, Bound<&[u8]>) = (
        Bound::Included(range.start.as_ref()),
        Bound::Excluded(range.end.as_ref()),
    );
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

#[async_trait]
impl Batch for RedbBatch {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        let key = key.to_vec();
        self.blocking(move |db| {
            let tx = db.begin_read().map_err(Error::backend)?;
            let table = tx.open_table(KV).map_err(Error::backend)?;
            let value = table
                .get(key.as_slice())
                .map_err(Error::backend)?
                .map(|guard| Bytes::copy_from_slice(guard.value()));
            Ok(value)
        })
        .await
    }

    async fn scan(&self, range: &RawRange) -> Result<Vec<(Bytes, Bytes)>> {
        if range.start >= range.end {
            return Ok(Vec::new());
        }
        let range = range.clone();
        self.blocking(move |db| {
            let tx = db.begin_read().map_err(Error::backend)?;
            let table = tx.open_table(KV).map_err(Error::backend)?;
            scan_table(&table, &range)
        })
        .await
    }

    async fn apply(&self, ops: Vec<Op>, stamp: bool) -> Result<Option<[u8; 10]>> {
        self.blocking(move |db| {
            let wtx = db.begin_write().map_err(Error::backend)?;
            let stamp = if stamp {
                let mut sys = wtx.open_table(SYS).map_err(Error::backend)?;
                let next = sys
                    .get(COMMITS)
                    .map_err(Error::backend)?
                    .map_or(0, |g| g.value())
                    + 1;
                sys.insert(COMMITS, next).map_err(Error::backend)?;
                Some(stamp_of(next))
            } else {
                None
            };
            let ops = resolve_stamps(ops, stamp.as_ref())?;
            {
                let mut table = wtx.open_table(KV).map_err(Error::backend)?;
                for op in ops {
                    match op {
                        Op::Set(k, v) | Op::SetStamped(k, v) => {
                            table
                                .insert(k.as_ref(), v.as_ref())
                                .map_err(Error::backend)?;
                        }
                        Op::Clear(k) => {
                            table.remove(k.as_ref()).map_err(Error::backend)?;
                        }
                        Op::ClearRange(start, end) => {
                            if start < end {
                                table
                                    .retain_in::<&[u8], _>(start.as_ref()..end.as_ref(), |_, _| {
                                        false
                                    })
                                    .map_err(Error::backend)?;
                            }
                        }
                        Op::Add(k, delta) => {
                            let sum = {
                                let old = table.get(k.as_ref()).map_err(Error::backend)?;
                                add_le(old.as_ref().map(|g| g.value()), delta)
                            };
                            table.insert(k.as_ref(), &sum[..]).map_err(Error::backend)?;
                        }
                    }
                }
            }
            wtx.commit().map_err(Error::backend)?;
            Ok(stamp)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::{MetaStore, Transaction};

    #[tokio::test]
    async fn values_and_stamps_survive_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("meta.redb");
        let first = {
            let store = RedbMetaStore::open(&path).await.unwrap();
            store
                .set(Bytes::from_static(b"k"), Bytes::from_static(b"v"))
                .await
                .unwrap();
            store
                .transact(Transaction::new().append_stamped(&b"l"[..], &b"1"[..]))
                .await
                .unwrap()
                .stamped[0]
        };
        let store = RedbMetaStore::open(&path).await.unwrap();
        assert_eq!(
            store.get(b"k").await.unwrap(),
            Some(Bytes::from_static(b"v"))
        );
        let second = store
            .transact(Transaction::new().append_stamped(&b"l"[..], &b"2"[..]))
            .await
            .unwrap()
            .stamped[0];
        assert!(second > first);
    }
}
