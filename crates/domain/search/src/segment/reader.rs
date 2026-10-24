//! Reading a segment: posting lists, contents and symbols on demand.

use std::collections::HashMap;

use rill_types::ObjectHash;

use super::bytes::SegmentBytes;
use super::format::{trigram_key, unpack_entry, Header, SegmentDoc, MAGIC};
use crate::error::{Error, Result};
use crate::symbols::Symbol;
use crate::trigram::{Candidates, PostingList, Postings, TrigramQuery};

/// An open segment.
pub struct Segment {
    bytes: SegmentBytes,
    data_start: usize,
    header: Header,
    dict: fst::Map<Vec<u8>>,
    by_hash: HashMap<ObjectHash, u32>,
}

impl std::fmt::Debug for Segment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Segment({} docs)", self.header.docs.len())
    }
}

fn corrupt(what: &str) -> Error {
    Error::Codec(format!("corrupt segment: {what}"))
}

impl Segment {
    /// Parse a segment's header and dictionary.
    pub fn open(bytes: SegmentBytes) -> Result<Segment> {
        if bytes.len() < 12 || &bytes[..8] != MAGIC {
            return Err(corrupt("bad magic"));
        }
        let header_len = u32::from_le_bytes(bytes[8..12].try_into().expect("four bytes")) as usize;
        let data_start = 12 + header_len;
        let header: Header = postcard::from_bytes(
            bytes
                .get(12..data_start)
                .ok_or_else(|| corrupt("short header"))?,
        )
        .map_err(|e| Error::Codec(format!("segment header: {e}")))?;
        let data = bytes.get(data_start..).ok_or_else(|| corrupt("no data"))?;
        let dict = fst::Map::new(header.dict.slice(data)?.to_vec())
            .map_err(|e| Error::Codec(format!("segment dictionary: {e}")))?;
        let by_hash = header
            .docs
            .iter()
            .enumerate()
            .map(|(i, d)| (d.hash, i as u32))
            .collect();
        Ok(Segment {
            bytes,
            data_start,
            header,
            dict,
            by_hash,
        })
    }

    fn data(&self) -> &[u8] {
        &self.bytes[self.data_start..]
    }

    /// The blobs in the segment, by number.
    pub fn docs(&self) -> &[SegmentDoc] {
        &self.header.docs
    }

    /// The number of the blob with `hash`, if the segment holds it.
    pub fn doc_of(&self, hash: &ObjectHash) -> Option<u32> {
        self.by_hash.get(hash).copied()
    }

    /// Bytes on disk.
    pub fn size(&self) -> usize {
        self.bytes.len()
    }

    /// The blobs containing `trigram`.
    pub fn postings(&self, trigram: u32) -> Result<Option<PostingList>> {
        let Some(entry) = self.dict.get(trigram_key(trigram)) else {
            return Ok(None);
        };
        let (offset, len) = unpack_entry(entry);
        let region = self.header.postings.slice(self.data())?;
        let raw = region
            .get(offset..offset + len)
            .ok_or_else(|| corrupt("posting list out of range"))?;
        // Segments are content-addressed and verified when fetched, so
        // their posting lists are read without re-validating them.
        PostingList::deserialize_unchecked_from(raw)
            .map(Some)
            .map_err(|e| Error::Codec(format!("posting list: {e}")))
    }

    /// The blobs `query` admits.
    pub fn candidates(&self, query: &TrigramQuery) -> Result<Candidates> {
        let lookup = SegmentPostings {
            segment: self,
            failed: std::cell::RefCell::new(None),
        };
        let out = query.evaluate(&lookup);
        match lookup.failed.into_inner() {
            Some(e) => Err(e),
            None => Ok(out),
        }
    }

    /// The content of blob `doc`.
    pub fn content(&self, doc: u32) -> Result<Vec<u8>> {
        let d = self
            .header
            .docs
            .get(doc as usize)
            .ok_or_else(|| corrupt("no such doc"))?;
        let region = self.header.contents.slice(self.data())?;
        let raw = d.content.slice(region)?;
        super::codec::decompress(raw, d.len as usize)
    }

    /// [`Segment::content`] into a buffer the caller reuses.
    pub fn content_into(&self, doc: u32, out: &mut Vec<u8>) -> Result<()> {
        let d = self
            .header
            .docs
            .get(doc as usize)
            .ok_or_else(|| corrupt("no such doc"))?;
        let region = self.header.contents.slice(self.data())?;
        super::codec::decompress_into(d.content.slice(region)?, d.len as usize, out)
    }

    /// The symbols of blob `doc`.
    pub fn symbols(&self, doc: u32) -> Result<Vec<Symbol>> {
        let d = self
            .header
            .docs
            .get(doc as usize)
            .ok_or_else(|| corrupt("no such doc"))?;
        let region = self.header.symbols.slice(self.data())?;
        postcard::from_bytes(d.symbols.slice(region)?)
            .map_err(|e| Error::Codec(format!("symbols: {e}")))
    }
}

/// A segment's posting lists, keeping the first read error.
struct SegmentPostings<'a> {
    segment: &'a Segment,
    failed: std::cell::RefCell<Option<Error>>,
}

impl Postings for SegmentPostings<'_> {
    fn size(&self, trigram: u32) -> Option<usize> {
        self.segment
            .dict
            .get(trigram_key(trigram))
            .map(|entry| unpack_entry(entry).1)
    }

    fn get(&self, trigram: u32) -> Option<PostingList> {
        match self.segment.postings(trigram) {
            Ok(p) => p,
            Err(e) => {
                self.failed.borrow_mut().get_or_insert(e);
                None
            }
        }
    }
}
