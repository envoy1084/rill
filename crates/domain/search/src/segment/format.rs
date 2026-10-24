//! The layout of a segment:
//!
//! ```text
//! "RILLSEG1" | header length (u32 LE) | header (postcard) |
//! dictionary (fst) | posting lists | contents | symbols
//! ```
//!
//! Offsets in the header are from the end of the header. The dictionary
//! maps each trigram (three bytes, big-endian, so the map is sorted by
//! trigram) to `offset << 24 | length` of its posting list.

use rill_types::ObjectHash;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub(super) const MAGIC: &[u8; 8] = b"RILLSEG1";
/// Longest posting list one dictionary entry can address.
pub(super) const MAX_POSTING_LEN: usize = (1 << 24) - 1;

/// A region of the data area.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Span {
    pub offset: u64,
    pub len: u64,
}

impl Span {
    pub(super) fn slice<'a>(&self, data: &'a [u8]) -> Result<&'a [u8]> {
        let (start, end) = (self.offset as usize, (self.offset + self.len) as usize);
        data.get(start..end)
            .ok_or_else(|| Error::Codec(format!("segment span {start}..{end} out of range")))
    }
}

/// One blob in a segment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentDoc {
    pub hash: ObjectHash,
    /// Uncompressed size.
    pub len: u32,
    pub(super) content: Span,
    pub(super) symbols: Span,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct Header {
    pub docs: Vec<SegmentDoc>,
    pub dict: Span,
    pub postings: Span,
    pub contents: Span,
    pub symbols: Span,
}

pub(super) fn pack_entry(offset: usize, len: usize) -> Result<u64> {
    if len > MAX_POSTING_LEN {
        return Err(Error::Codec(format!("posting list of {len} bytes")));
    }
    Ok(((offset as u64) << 24) | len as u64)
}

pub(super) fn unpack_entry(v: u64) -> (usize, usize) {
    ((v >> 24) as usize, (v & MAX_POSTING_LEN as u64) as usize)
}

pub(super) fn trigram_key(t: u32) -> [u8; 3] {
    crate::trigram::unpack(t)
}
