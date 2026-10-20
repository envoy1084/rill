//! Metadata store on FoundationDB, for multi-node deployments.
//!
//! A [`Transaction`] maps onto one FDB transaction executed through
//! [`Database::run`], which retries on conflicts and transient errors. Each
//! condition is a read inside that transaction, so FDB's serialisable
//! isolation guarantees the check and the writes commit together: if any
//! key a condition read changes before commit, the commit fails and the
//! closure runs again against a fresh snapshot.
//!
//! `AppendLog` finds the next sequence number with a reverse range read of
//! one key over the prefix. That read puts the whole tail of the log in the
//! transaction's read conflict range, so concurrent appenders to the same
//! log serialise, which is also what gives gap-free numbering. FDB's
//! versionstamped keys (`SetVersionstampedKey`) would make appends
//! conflict-free, but sequences would then be 80-bit versionstamps instead
//! of dense `u64`s that trunk and the change log rely on; switching is a
//! future optimisation for hot logs, not a correctness change.
//!
//! The crate links against `libfdb_c`; the network thread must be started
//! once per process with [`foundationdb::boot`] before any store is used.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use foundationdb::{
    Database, FdbBindingError, KeySelector, RangeOption, RetryableTransaction, Transaction as FdbTx,
};
use futures::TryStreamExt;

use crate::error::{Error, Result};
use crate::memory::{check_conditions, next_seq};
use crate::meta::{
    prefix_end, seq_key, Condition, KeyRange, MetaStore, Mutation, Transaction, TxOutcome,
};

/// [`MetaStore`] backed by a FoundationDB cluster.
#[derive(Clone)]
pub struct FdbMetaStore {
    db: Arc<Database>,
    /// Byte prefix every key is stored under; lets several Rill
    /// deployments share one cluster.
    prefix: Bytes,
}

impl FdbMetaStore {
    /// Connect using the cluster file at `cluster_file` (or the default
    /// location when `None`) and store keys under `prefix`.
    pub fn open(cluster_file: Option<&str>, prefix: impl Into<Bytes>) -> Result<Self> {
        let db = Database::new(cluster_file).map_err(Error::backend)?;
        Ok(Self::from_database(db, prefix))
    }

    /// Wrap an existing database handle.
    pub fn from_database(db: Database, prefix: impl Into<Bytes>) -> Self {
        FdbMetaStore {
            db: Arc::new(db),
            prefix: prefix.into(),
        }
    }

    fn key(&self, key: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.prefix.len() + key.len());
        out.extend_from_slice(&self.prefix);
        out.extend_from_slice(key);
        out
    }

    fn strip(&self, key: &[u8]) -> Bytes {
        Bytes::copy_from_slice(&key[self.prefix.len()..])
    }

    /// Translate a [`KeyRange`] into FDB key selectors over the prefixed
    /// keyspace. FDB ranges are always `[begin, end)`, so inclusive and
    /// exclusive bounds become `first_greater_or_equal` / `first_greater_than`
    /// selectors, and an unbounded end becomes the end of the prefix.
    fn selectors(&self, range: &KeyRange) -> (KeySelector<'static>, KeySelector<'static>) {
        use std::ops::Bound;
        let begin = match &range.start {
            Bound::Included(k) => KeySelector::first_greater_or_equal(self.key(k)),
            Bound::Excluded(k) => KeySelector::first_greater_than(self.key(k)),
            Bound::Unbounded => KeySelector::first_greater_or_equal(self.prefix.to_vec()),
        };
        let end = match &range.end {
            Bound::Included(k) => KeySelector::first_greater_than(self.key(k)),
            Bound::Excluded(k) => KeySelector::first_greater_or_equal(self.key(k)),
            Bound::Unbounded => match prefix_end(&self.prefix) {
                Some(e) => KeySelector::first_greater_or_equal(e.to_vec()),
                // An all-0xFF prefix has no successor; FDB's user keyspace
                // ends at 0xFF, so this is the real end.
                None => KeySelector::first_greater_or_equal(vec![0xFF]),
            },
        };
        (begin, end)
    }

    /// Range read honouring limit and direction. `get_ranges_keyvalues`
    /// pages through large results itself, so one call stays within FDB's
    /// per-request limits whatever the range size.
    async fn scan_in(&self, trx: &FdbTx, range: &KeyRange) -> Result<Vec<(Bytes, Bytes)>> {
        if range.limit == 0 {
            return Ok(Vec::new());
        }
        let (begin, end) = self.selectors(range);
        let opt = RangeOption {
            begin,
            end,
            limit: (range.limit < usize::MAX).then_some(range.limit),
            reverse: range.reverse,
            ..RangeOption::default()
        };
        let mut out = Vec::new();
        let mut stream = trx.get_ranges_keyvalues(opt, false);
        while let Some(kv) = stream.try_next().await.map_err(Error::backend)? {
            out.push((self.strip(kv.key()), Bytes::copy_from_slice(kv.value())));
            if out.len() >= range.limit {
                break;
            }
        }
        Ok(out)
    }

    async fn get_in(&self, trx: &FdbTx, key: &[u8]) -> Result<Option<Bytes>> {
        let value = trx
            .get(&self.key(key), false)
            .await
            .map_err(Error::backend)?;
        Ok(value.map(|v| Bytes::copy_from_slice(&v)))
    }

    /// Body of a `transact` attempt. Runs inside `Database::run`, so it may
    /// execute more than once; everything here is a pure function of the
    /// snapshot plus buffered writes, which makes retries safe.
    async fn apply(&self, trx: &RetryableTransaction, tx: &Transaction) -> Result<TxOutcome> {
        let mut current = std::collections::BTreeMap::new();
        for cond in &tx.conditions {
            let key = match cond {
                Condition::Equals { key, .. }
                | Condition::Absent { key }
                | Condition::Present { key } => key,
            };
            if !current.contains_key(key) {
                let value = self.get_in(trx, key).await?;
                current.insert(key.clone(), value);
            }
        }
        check_conditions(&tx.conditions, |k| current.get(k).cloned().flatten())?;

        let mut outcome = TxOutcome::default();
        for m in &tx.mutations {
            match m {
                Mutation::Set { key, value } => trx.set(&self.key(key), value),
                Mutation::Delete { key } => trx.clear(&self.key(key)),
                Mutation::AppendLog { prefix, value } => {
                    // Writes buffered earlier in this transaction are visible
                    // to this read (FDB reads its own writes), so two appends
                    // to one prefix in a single transaction number correctly.
                    let last = self
                        .scan_in(trx, &KeyRange::prefix(prefix.clone()).reverse().limit(1))
                        .await?
                        .pop()
                        .map(|(k, _)| k);
                    let seq = next_seq(prefix, last.as_deref());
                    trx.set(&self.key(&seq_key(prefix, seq)), value);
                    outcome.appended.push(seq);
                }
            }
        }
        Ok(outcome)
    }
}

/// Carry a store error through `Database::run`, which only retries FDB
/// errors; anything of ours (a failed condition, a codec error) comes back
/// unchanged and unretried.
fn wrap(e: Error) -> FdbBindingError {
    FdbBindingError::CustomError(Box::new(e))
}

fn unwrap(e: FdbBindingError) -> Error {
    match e {
        FdbBindingError::CustomError(inner) => match inner.downcast::<Error>() {
            Ok(ours) => *ours,
            Err(other) => Error::Backend(other.to_string()),
        },
        other => Error::Backend(other.to_string()),
    }
}

#[async_trait]
impl MetaStore for FdbMetaStore {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        self.db
            .run(|trx, _| async move { self.get_in(&trx, key).await.map_err(wrap) })
            .await
            .map_err(unwrap)
    }

    async fn get_many(&self, keys: &[Bytes]) -> Result<Vec<Option<Bytes>>> {
        self.db
            .run(|trx, _| async move {
                let futures = keys.iter().map(|k| self.get_in(&trx, k));
                futures::future::try_join_all(futures).await.map_err(wrap)
            })
            .await
            .map_err(unwrap)
    }

    async fn scan(&self, range: KeyRange) -> Result<Vec<(Bytes, Bytes)>> {
        let range = &range;
        self.db
            .run(|trx, _| async move { self.scan_in(&trx, range).await.map_err(wrap) })
            .await
            .map_err(unwrap)
    }

    async fn transact(&self, tx: Transaction) -> Result<TxOutcome> {
        let tx = &tx;
        self.db
            .run(|trx, _| async move { self.apply(&trx, tx).await.map_err(wrap) })
            .await
            .map_err(unwrap)
    }
}
