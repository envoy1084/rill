//! How logical keys and values map onto engine rows.
//!
//! Three regions sit side by side: values, the chunks of values too large
//! for one row, and counters. A value row starts with a header byte: `0`
//! for the value inline, `1` for a split value whose chunks live in the
//! chunk region under `(key, id, index)`. The id is a digest of the whole
//! value, so a reader that finds a complete, matching set of chunks has
//! read exactly the value the header describes, even outside a
//! transaction.

use std::ops::Bound;
use std::time::Duration;

use bytes::{Buf, Bytes};
use rill_types::{ObjectHash, ObjectKind};

use super::engine::RawRange;
use crate::meta::{prefix_end, KeyRange};

const DATA: u8 = 0x01;
const CHUNKS: u8 = 0x02;
const COUNTERS: u8 = 0x03;

const INLINE: u8 = 0;
const SPLIT: u8 = 1;

/// Largest logical key. FoundationDB allows 10,000 bytes.
pub const MAX_KEY_BYTES: usize = 8 * 1024;
/// Largest engine row value. FoundationDB allows 100,000 bytes.
pub const ROW_BYTES: usize = 90_000;
/// Most bytes one transaction may write. FoundationDB allows 10 MB.
pub const TX_BYTES: usize = 1_000_000;
/// Transactions slower than this are logged.
pub const TX_TIME: Duration = Duration::from_secs(1);

pub fn data_key(key: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(key.len() + 1);
    out.push(DATA);
    out.extend_from_slice(key);
    out
}

pub fn counter_key(key: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(key.len() + 1);
    out.push(COUNTERS);
    out.extend_from_slice(key);
    out
}

/// The logical key of a value row. Advancing, unlike slicing, never
/// allocates.
pub fn logical(mut raw: Bytes) -> Bytes {
    raw.advance(1);
    raw
}

/// The chunk rows of every version of `key`.
pub fn chunks_of(key: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut start = vec![CHUNKS];
    foundationdb_tuple::pack_into(&key, &mut start);
    let end = prefix_end(&start)
        .expect("a chunk prefix starts with 0x02")
        .to_vec();
    (start, end)
}

/// The scan that finds the chunks of a split value.
pub fn chunk_scan(key: &[u8], id: &[u8; 8]) -> RawRange {
    let mut start = vec![CHUNKS];
    foundationdb_tuple::pack_into(&(key, &id[..]), &mut start);
    let end = prefix_end(&start).expect("a chunk prefix starts with 0x02");
    RawRange {
        start: Bytes::from(start),
        end,
        limit: usize::MAX,
        reverse: false,
    }
}

fn chunk_key(key: &[u8], id: &[u8], index: u32) -> Vec<u8> {
    let mut out = vec![CHUNKS];
    foundationdb_tuple::pack_into(&(key, id, index), &mut out);
    out
}

/// A value row whose value is inline.
pub fn inline_row(value: &[u8]) -> Vec<u8> {
    let mut row = Vec::with_capacity(value.len() + 1);
    row.push(INLINE);
    row.extend_from_slice(value);
    row
}

/// Whether `value` fits in its value row.
pub fn fits_inline(value: &[u8]) -> bool {
    value.len() < ROW_BYTES
}

fn split_id(value: &[u8]) -> [u8; 8] {
    let digest = ObjectHash::digest(ObjectKind::Derived, value);
    digest.as_bytes()[..8]
        .try_into()
        .expect("a digest has 32 bytes")
}

/// The rows of a value too large for one row: the header row and its
/// chunks, as `(key, value)` pairs.
pub fn split_rows(key: &[u8], value: &Bytes) -> (Vec<u8>, Vec<(Vec<u8>, Bytes)>) {
    let id = split_id(value);
    let chunks: Vec<(Vec<u8>, Bytes)> = (0..value.len())
        .step_by(ROW_BYTES)
        .enumerate()
        .map(|(i, start)| {
            let end = (start + ROW_BYTES).min(value.len());
            (chunk_key(key, &id, i as u32), value.slice(start..end))
        })
        .collect();
    let mut header = vec![SPLIT];
    header.extend_from_slice(&id);
    header.extend_from_slice(&(chunks.len() as u32).to_be_bytes());
    header.extend_from_slice(&(value.len() as u64).to_be_bytes());
    (header, chunks)
}

/// What a value row says about its value.
pub enum Row {
    Inline(Bytes),
    Split { id: [u8; 8], chunks: u32, len: u64 },
}

/// Parse a value row.
pub fn parse_row(mut row: Bytes) -> Option<Row> {
    match *row.first()? {
        INLINE => {
            row.advance(1);
            Some(Row::Inline(row))
        }
        SPLIT if row.len() == 21 => Some(Row::Split {
            id: row[1..9].try_into().ok()?,
            chunks: u32::from_be_bytes(row[9..13].try_into().ok()?),
            len: u64::from_be_bytes(row[13..21].try_into().ok()?),
        }),
        _ => None,
    }
}

/// Join chunks read for a split value, or `None` if they do not form the
/// value the header describes (it was rewritten while being read).
pub fn join_chunks(
    id: &[u8; 8],
    chunks: u32,
    len: u64,
    rows: Vec<(Bytes, Bytes)>,
) -> Option<Bytes> {
    if rows.len() != chunks as usize {
        return None;
    }
    let mut value = Vec::with_capacity(usize::try_from(len).ok()?);
    for (_, part) in rows {
        value.extend_from_slice(&part);
    }
    (value.len() as u64 == len && &split_id(&value) == id).then(|| Bytes::from(value))
}

/// The engine range holding the values of a logical range, or `None` when
/// the logical range is empty.
pub fn data_range(range: &KeyRange) -> Option<RawRange> {
    let after = |k: &Bytes| {
        let mut v = data_key(k);
        v.push(0);
        v
    };
    let start = match &range.start {
        Bound::Included(k) => data_key(k),
        Bound::Excluded(k) => after(k),
        Bound::Unbounded => vec![DATA],
    };
    let end = match &range.end {
        Bound::Included(k) => after(k),
        Bound::Excluded(k) => data_key(k),
        Bound::Unbounded => vec![DATA + 1],
    };
    (start < end && range.limit > 0).then(|| RawRange {
        start: Bytes::from(start),
        end: Bytes::from(end),
        limit: range.limit,
        reverse: range.reverse,
    })
}

/// The logical key a versionstamped append writes, with the engine's
/// placeholder and offset suffix.
pub fn stamped_data_key(prefix: &[u8], user_version: u16) -> Vec<u8> {
    let mut key = data_key(prefix);
    foundationdb_tuple::pack_into_with_versionstamp(
        &foundationdb_tuple::Versionstamp::incomplete(user_version),
        &mut key,
    );
    key
}

/// Printable form of a key for error messages.
pub fn show(key: &[u8]) -> String {
    key.escape_ascii().to_string()
}
