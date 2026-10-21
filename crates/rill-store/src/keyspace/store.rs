//! [`MetaStore`] on any [`Engine`]: conditions, logs, stamps, counters,
//! value splitting and the transaction budget, implemented once.

use std::collections::HashMap;
use std::time::Instant;

use async_trait::async_trait;
use bytes::Bytes;
use rill_types::Versionstamp;

use super::engine::{Engine, EngineTxn, RawRange};
use super::layout::{self, Row};
use crate::error::{Error, Result};
use crate::meta::{
    prefix_end, seq_from_key, seq_key, Condition, KeyRange, MetaStore, Mutation, Transaction,
    TxOutcome,
};

/// Attempts at a transaction that keeps failing with a retryable error.
const MAX_ATTEMPTS: usize = 64;
/// Re-reads of a split value that keeps changing under an outside read.
const MAX_REREADS: usize = 16;

/// The metadata store over an engine.
pub struct Keyspace<E> {
    engine: E,
}

impl<E: Engine> Keyspace<E> {
    /// A store over `engine`.
    pub fn with_engine(engine: E) -> Self {
        Keyspace { engine }
    }

    /// The engine underneath.
    pub fn engine(&self) -> &E {
        &self.engine
    }

    async fn get_outside(&self, key: &[u8]) -> Result<Option<Bytes>> {
        let raw_key = layout::data_key(key);
        for _ in 0..MAX_REREADS {
            let Some(row) = self.engine.get(&raw_key).await? else {
                return Ok(None);
            };
            if let Some(value) = self.resolve_outside(key, row).await? {
                return Ok(Some(value));
            }
        }
        Err(Error::Conflict(format!(
            "{} kept changing while being read",
            layout::show(key)
        )))
    }

    /// The value a row describes, or `None` if its chunks were replaced
    /// between reading the row and reading them.
    async fn resolve_outside(&self, key: &[u8], row: Bytes) -> Result<Option<Bytes>> {
        match parse(key, row)? {
            Row::Inline(value) => Ok(Some(value)),
            Row::Split { id, chunks, len } => {
                let rows = self.engine.scan(&layout::chunk_scan(key, &id)).await?;
                Ok(layout::join_chunks(&id, chunks, len, rows))
            }
        }
    }
}

fn parse(key: &[u8], row: Bytes) -> Result<Row> {
    layout::parse_row(row)
        .ok_or_else(|| Error::Codec(format!("malformed row at {}", layout::show(key))))
}

async fn get_in<T: EngineTxn>(txn: &mut T, key: &[u8]) -> Result<Option<Bytes>> {
    let Some(row) = txn.get(&layout::data_key(key)).await? else {
        return Ok(None);
    };
    match parse(key, row)? {
        Row::Inline(value) => Ok(Some(value)),
        Row::Split { id, chunks, len } => {
            let rows = txn.scan(&layout::chunk_scan(key, &id)).await?;
            layout::join_chunks(&id, chunks, len, rows)
                .map(Some)
                .ok_or_else(|| Error::Codec(format!("torn value at {}", layout::show(key))))
        }
    }
}

async fn last_seq<T: EngineTxn>(txn: &mut T, prefix: &[u8]) -> Result<u64> {
    let start = layout::data_key(prefix);
    let Some(end) = prefix_end(&start) else {
        return Ok(0);
    };
    let range = RawRange {
        start: Bytes::from(start.clone()),
        end,
        limit: 1,
        reverse: true,
    };
    match txn.scan(&range).await?.first() {
        Some((key, _)) => seq_from_key(start.len(), key).ok_or_else(|| {
            Error::Codec(format!(
                "{} holds a key that is not a log entry",
                layout::show(prefix)
            ))
        }),
        None => Ok(0),
    }
}

fn put<T: EngineTxn>(txn: &mut T, key: &[u8], value: &Bytes) {
    let (start, end) = layout::chunks_of(key);
    txn.clear_range(start, end);
    if layout::fits_inline(value) {
        txn.set(
            layout::data_key(key),
            Bytes::from(layout::inline_row(value)),
        );
    } else {
        let (header, chunks) = layout::split_rows(key, value);
        for (chunk_key, part) in chunks {
            txn.set(chunk_key, part);
        }
        txn.set(layout::data_key(key), Bytes::from(header));
    }
}

fn delete<T: EngineTxn>(txn: &mut T, key: &[u8]) {
    let (start, end) = layout::chunks_of(key);
    txn.clear_range(start, end);
    txn.clear(layout::data_key(key));
}

fn conflict(cond: &Condition) -> Error {
    let (key, what) = match cond {
        Condition::Equals { key, .. } => (key, "changed"),
        Condition::Absent { key } => (key, "exists"),
        Condition::Present { key } => (key, "is missing"),
    };
    Error::Conflict(format!("key {} {what}", layout::show(key)))
}

/// Refuse a transaction that breaks a size limit before touching the
/// engine.
fn check_budget(tx: &Transaction) -> Result<()> {
    let too_large = |what: String| Err(Error::TooLarge(what));
    let mut bytes = 0usize;
    let mut stamped = 0usize;
    let keys = tx.conditions.iter().map(|c| match c {
        Condition::Equals { key, .. } | Condition::Absent { key } | Condition::Present { key } => {
            key
        }
    });
    for key in keys {
        if key.len() > layout::MAX_KEY_BYTES {
            return too_large(format!("key of {} bytes", key.len()));
        }
    }
    for m in &tx.mutations {
        let (key, value) = match m {
            Mutation::Set { key, value } => (key, value.len()),
            Mutation::Delete { key } => (key, 0),
            Mutation::AppendLog { prefix, value } => (prefix, value.len() + 9),
            Mutation::AppendStamped { prefix, value } => {
                stamped += 1;
                if !layout::fits_inline(value) {
                    return too_large(format!("stamped value of {} bytes", value.len()));
                }
                (prefix, value.len() + 13)
            }
            Mutation::Add { key, .. } => (key, 8),
        };
        if key.len() > layout::MAX_KEY_BYTES {
            return too_large(format!("key of {} bytes", key.len()));
        }
        bytes += key.len() + value;
    }
    if bytes > layout::TX_BYTES {
        return too_large(format!(
            "transaction writes {bytes} bytes; the limit is {}",
            layout::TX_BYTES
        ));
    }
    if stamped > usize::from(u16::MAX) {
        return too_large(format!("{stamped} stamped appends in one transaction"));
    }
    Ok(())
}

async fn attempt<T: EngineTxn>(txn: &mut T, tx: &Transaction) -> Result<TxOutcome> {
    for cond in &tx.conditions {
        let holds = match cond {
            Condition::Equals { key, value } => get_in(txn, key).await? == *value,
            Condition::Absent { key } => get_in(txn, key).await?.is_none(),
            Condition::Present { key } => get_in(txn, key).await?.is_some(),
        };
        if !holds {
            return Err(conflict(cond));
        }
    }
    let mut next: HashMap<&Bytes, u64> = HashMap::new();
    for m in &tx.mutations {
        if let Mutation::AppendLog { prefix, .. } = m {
            if !next.contains_key(prefix) {
                let last = last_seq(txn, prefix).await?;
                next.insert(prefix, last + 1);
            }
        }
    }
    let mut outcome = TxOutcome::default();
    let mut user_versions = Vec::new();
    for m in &tx.mutations {
        match m {
            Mutation::Set { key, value } => put(txn, key, value),
            Mutation::Delete { key } => delete(txn, key),
            Mutation::AppendLog { prefix, value } => {
                let seq = next.get_mut(prefix).expect("every log prefix was read");
                put(txn, &seq_key(prefix, *seq), value);
                outcome.appended.push(*seq);
                *seq += 1;
            }
            Mutation::AppendStamped { prefix, value } => {
                let user_version = user_versions.len() as u16;
                txn.set_versionstamped_key(
                    layout::stamped_data_key(prefix, user_version),
                    Bytes::from(layout::inline_row(value)),
                );
                user_versions.push(user_version);
            }
            Mutation::Add { key, delta } => txn.add(layout::counter_key(key), *delta),
        }
    }
    let stamp = txn.commit().await?;
    if !user_versions.is_empty() {
        let stamp =
            stamp.ok_or_else(|| Error::Backend("a stamped commit returned no stamp".into()))?;
        outcome.stamped = user_versions
            .into_iter()
            .map(|user| {
                let mut bytes = [0u8; 12];
                bytes[..10].copy_from_slice(&stamp);
                bytes[10..].copy_from_slice(&user.to_be_bytes());
                Versionstamp(bytes)
            })
            .collect();
    }
    Ok(outcome)
}

#[async_trait]
impl<E: Engine> MetaStore for Keyspace<E> {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        self.get_outside(key).await
    }

    async fn scan(&self, range: KeyRange) -> Result<Vec<(Bytes, Bytes)>> {
        let Some(raw) = layout::data_range(&range) else {
            return Ok(Vec::new());
        };
        let rows = self.engine.scan(&raw).await?;
        let mut out = Vec::with_capacity(rows.len());
        for (raw_key, row) in rows {
            let key = layout::logical(raw_key);
            let value = match parse(&key, row)? {
                Row::Inline(value) => Some(value),
                Row::Split { id, chunks, len } => {
                    let parts = self.engine.scan(&layout::chunk_scan(&key, &id)).await?;
                    match layout::join_chunks(&id, chunks, len, parts) {
                        Some(value) => Some(value),
                        None => self.get_outside(&key).await?,
                    }
                }
            };
            if let Some(value) = value {
                out.push((key, value));
            }
        }
        Ok(out)
    }

    async fn transact(&self, tx: Transaction) -> Result<TxOutcome> {
        check_budget(&tx)?;
        if tx.is_empty() {
            return Ok(TxOutcome::default());
        }
        let started = Instant::now();
        let mut txn = self.engine.begin().await?;
        let mut attempts = 1;
        let result = loop {
            match attempt(&mut txn, &tx).await {
                Err(e @ Error::Retryable(_)) if attempts < MAX_ATTEMPTS => {
                    attempts += 1;
                    txn.reset_after(&e).await?;
                }
                other => break other,
            }
        };
        let elapsed = started.elapsed();
        if elapsed > layout::TX_TIME {
            tracing::warn!(
                elapsed_ms = elapsed.as_millis() as u64,
                attempts,
                mutations = tx.mutations.len(),
                "slow metadata transaction"
            );
        }
        result
    }

    async fn counter(&self, key: &[u8]) -> Result<i64> {
        let raw = self.engine.get(&layout::counter_key(key)).await?;
        Ok(i64::from_le_bytes(super::engine::add_le(raw.as_deref(), 0)))
    }

    async fn wait_counter(
        &self,
        key: &[u8],
        seen: i64,
        timeout: std::time::Duration,
    ) -> Result<i64> {
        let raw_key = layout::counter_key(key);
        let raw = self.engine.get(&raw_key).await?;
        let value = i64::from_le_bytes(super::engine::add_le(raw.as_deref(), 0));
        if value != seen {
            return Ok(value);
        }
        self.engine.wait_change(&raw_key, raw, timeout).await?;
        self.counter(key).await
    }
}
