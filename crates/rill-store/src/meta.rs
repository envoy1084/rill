//! The transactional metadata store.
//!
//! A minimal ordered key-value interface. Writes are batched into a
//! [`Transaction`] of conditions and mutations that commits atomically or
//! not at all. Every backend implements it through the shared
//! [`Keyspace`](crate::keyspace::Keyspace) layer, so memory, redb and
//! FoundationDB agree on every rule.

use std::ops::Bound;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;

use rill_types::Versionstamp;

use crate::error::Result;

/// A precondition checked at commit time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Condition {
    /// The key's current value must equal `value` (`None`: must be absent).
    Equals { key: Bytes, value: Option<Bytes> },
    /// The key must be absent.
    Absent { key: Bytes },
    /// The key must be present.
    Present { key: Bytes },
}

/// A write applied if all conditions hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mutation {
    Set {
        key: Bytes,
        value: Bytes,
    },
    Delete {
        key: Bytes,
    },
    /// Append `value` under `prefix` with the next sequence number for that
    /// prefix, producing key [`seq_key`]`(prefix, seq)`. Sequences are dense,
    /// so concurrent appenders to one prefix conflict; use it for logs with
    /// one writer at a time. The sequence is reported in
    /// [`TxOutcome::appended`].
    AppendLog {
        prefix: Bytes,
        value: Bytes,
    },
    /// Append `value` under `prefix` keyed by the commit's versionstamp,
    /// producing key [`stamp_key`]`(prefix, stamp)`. Appenders never
    /// conflict. The stamp is reported in [`TxOutcome::stamped`]; the value
    /// must fit in one row.
    AppendStamped {
        prefix: Bytes,
        value: Bytes,
    },
    /// Add `delta` to the counter at `key` without reading it, so
    /// concurrent adds never conflict. Counters live apart from values and
    /// are read with [`MetaStore::counter`].
    Add {
        key: Bytes,
        delta: i64,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Transaction {
    pub conditions: Vec<Condition>,
    pub mutations: Vec<Mutation>,
}

impl Transaction {
    pub fn new() -> Self {
        Transaction::default()
    }

    pub fn expect(mut self, key: impl Into<Bytes>, value: Option<Bytes>) -> Self {
        self.conditions.push(Condition::Equals {
            key: key.into(),
            value,
        });
        self
    }

    pub fn expect_absent(mut self, key: impl Into<Bytes>) -> Self {
        self.conditions.push(Condition::Absent { key: key.into() });
        self
    }

    pub fn expect_present(mut self, key: impl Into<Bytes>) -> Self {
        self.conditions.push(Condition::Present { key: key.into() });
        self
    }

    pub fn set(mut self, key: impl Into<Bytes>, value: impl Into<Bytes>) -> Self {
        self.mutations.push(Mutation::Set {
            key: key.into(),
            value: value.into(),
        });
        self
    }

    pub fn delete(mut self, key: impl Into<Bytes>) -> Self {
        self.mutations.push(Mutation::Delete { key: key.into() });
        self
    }

    pub fn append_log(mut self, prefix: impl Into<Bytes>, value: impl Into<Bytes>) -> Self {
        self.mutations.push(Mutation::AppendLog {
            prefix: prefix.into(),
            value: value.into(),
        });
        self
    }

    pub fn append_stamped(mut self, prefix: impl Into<Bytes>, value: impl Into<Bytes>) -> Self {
        self.mutations.push(Mutation::AppendStamped {
            prefix: prefix.into(),
            value: value.into(),
        });
        self
    }

    pub fn add(mut self, key: impl Into<Bytes>, delta: i64) -> Self {
        self.mutations.push(Mutation::Add {
            key: key.into(),
            delta,
        });
        self
    }

    /// Add `other`'s conditions and mutations, so separately built parts
    /// commit as one transaction.
    pub fn merge(mut self, other: Transaction) -> Self {
        self.conditions.extend(other.conditions);
        self.mutations.extend(other.mutations);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.conditions.is_empty() && self.mutations.is_empty()
    }
}

/// Result of a committed transaction.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TxOutcome {
    /// Sequence numbers assigned to `AppendLog` mutations, in order.
    pub appended: Vec<u64>,
    /// Stamps assigned to `AppendStamped` mutations, in order.
    pub stamped: Vec<Versionstamp>,
}

/// A key range for scans. `end` is exclusive.
#[derive(Debug, Clone)]
pub struct KeyRange {
    pub start: Bound<Bytes>,
    pub end: Bound<Bytes>,
    pub limit: usize,
    pub reverse: bool,
}

impl KeyRange {
    /// All keys beginning with `prefix`.
    pub fn prefix(prefix: impl Into<Bytes>) -> Self {
        let prefix: Bytes = prefix.into();
        let end = prefix_end(&prefix);
        KeyRange {
            start: Bound::Included(prefix),
            end: match end {
                Some(e) => Bound::Excluded(e),
                None => Bound::Unbounded,
            },
            limit: usize::MAX,
            reverse: false,
        }
    }

    pub fn between(start: impl Into<Bytes>, end: impl Into<Bytes>) -> Self {
        KeyRange {
            start: Bound::Included(start.into()),
            end: Bound::Excluded(end.into()),
            limit: usize::MAX,
            reverse: false,
        }
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    pub fn reverse(mut self) -> Self {
        self.reverse = true;
        self
    }

    pub fn contains(&self, key: &[u8]) -> bool {
        let after_start = match &self.start {
            Bound::Included(s) => key >= s.as_ref(),
            Bound::Excluded(s) => key > s.as_ref(),
            Bound::Unbounded => true,
        };
        let before_end = match &self.end {
            Bound::Included(e) => key <= e.as_ref(),
            Bound::Excluded(e) => key < e.as_ref(),
            Bound::Unbounded => true,
        };
        after_start && before_end
    }
}

/// The smallest key greater than every key with the given prefix, or `None`
/// if the prefix is all 0xFF.
pub fn prefix_end(prefix: &[u8]) -> Option<Bytes> {
    let mut end = prefix.to_vec();
    while let Some(last) = end.pop() {
        if last < 0xFF {
            end.push(last + 1);
            return Some(Bytes::from(end));
        }
    }
    None
}

/// Ordered, transactional key-value storage.
#[async_trait]
pub trait MetaStore: Send + Sync + 'static {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>>;

    async fn get_many(&self, keys: &[Bytes]) -> Result<Vec<Option<Bytes>>> {
        let mut out = Vec::with_capacity(keys.len());
        for k in keys {
            out.push(self.get(k).await?);
        }
        Ok(out)
    }

    async fn scan(&self, range: KeyRange) -> Result<Vec<(Bytes, Bytes)>>;

    /// Commit atomically. Returns [`Error::Conflict`](crate::Error::Conflict)
    /// if a condition fails.
    async fn transact(&self, tx: Transaction) -> Result<TxOutcome>;

    /// The value of a counter written by [`Mutation::Add`]; 0 if absent.
    async fn counter(&self, key: &[u8]) -> Result<i64>;

    /// Unconditional single-key write; a convenience over `transact`.
    async fn set(&self, key: Bytes, value: Bytes) -> Result<()> {
        self.transact(Transaction::new().set(key, value)).await?;
        Ok(())
    }
}

#[async_trait]
impl<T: MetaStore + ?Sized> MetaStore for Arc<T> {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        (**self).get(key).await
    }

    async fn get_many(&self, keys: &[Bytes]) -> Result<Vec<Option<Bytes>>> {
        (**self).get_many(keys).await
    }

    async fn scan(&self, range: KeyRange) -> Result<Vec<(Bytes, Bytes)>> {
        (**self).scan(range).await
    }

    async fn transact(&self, tx: Transaction) -> Result<TxOutcome> {
        (**self).transact(tx).await
    }

    async fn counter(&self, key: &[u8]) -> Result<i64> {
        (**self).counter(key).await
    }
}

/// The key of entry `seq` in a log under `prefix`: the prefix followed by
/// the tuple encoding of `seq`, which sorts numerically.
pub fn seq_key(prefix: &[u8], seq: u64) -> Bytes {
    let mut key = prefix.to_vec();
    foundationdb_tuple::pack_into(&seq, &mut key);
    Bytes::from(key)
}

/// The sequence number of a key produced by [`seq_key`].
pub fn seq_from_key(prefix_len: usize, key: &[u8]) -> Option<u64> {
    foundationdb_tuple::unpack(key.get(prefix_len..)?).ok()
}

/// The key of the entry stamped `stamp` in a log under `prefix`.
pub fn stamp_key(prefix: &[u8], stamp: &Versionstamp) -> Bytes {
    let mut key = prefix.to_vec();
    let (tr, user) = stamp.as_bytes().split_at(10);
    let tr: [u8; 10] = tr.try_into().expect("a stamp splits into 10 and 2 bytes");
    let stamp =
        foundationdb_tuple::Versionstamp::complete(tr, u16::from_be_bytes([user[0], user[1]]));
    foundationdb_tuple::pack_into(&stamp, &mut key);
    Bytes::from(key)
}

/// The stamp of a key produced by [`stamp_key`].
pub fn stamp_from_key(prefix_len: usize, key: &[u8]) -> Option<Versionstamp> {
    let stamp: foundationdb_tuple::Versionstamp =
        foundationdb_tuple::unpack(key.get(prefix_len..)?).ok()?;
    Some(Versionstamp(*stamp.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_end_increments() {
        assert_eq!(prefix_end(b"ab").unwrap().as_ref(), b"ac");
        assert_eq!(prefix_end(&[0x61, 0xFF]).unwrap().as_ref(), &[0x62]);
        assert!(prefix_end(&[0xFF, 0xFF]).is_none());
    }

    #[test]
    fn seq_keys_sort_numerically() {
        let keys: Vec<Bytes> = [0, 9, 255, 256, 70_000, u64::MAX]
            .into_iter()
            .map(|s| seq_key(b"l/", s))
            .collect();
        assert!(keys.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(seq_from_key(2, &keys[3]), Some(256));
    }
}
