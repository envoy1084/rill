//! Building a segment from blobs.

use std::collections::BTreeMap;

use bytes::{BufMut, Bytes, BytesMut};
use rill_types::ObjectHash;

use super::format::{pack_entry, trigram_key, Header, SegmentDoc, Span, MAGIC};
use crate::error::{Error, Result};
use crate::symbols::Symbol;
use crate::trigram::{trigrams, PostingList};

/// One blob made ready for a segment: tokenized and compressed, the
/// CPU-heavy part, done off the async threads.
pub struct PreparedDoc {
    hash: ObjectHash,
    trigrams: Vec<u32>,
    compressed: Vec<u8>,
    symbols: Vec<u8>,
    len: u32,
}

impl PreparedDoc {
    /// Tokenize and compress `content`.
    pub fn new(hash: ObjectHash, content: &[u8], symbols: &[Symbol]) -> Result<PreparedDoc> {
        Ok(PreparedDoc {
            hash,
            trigrams: trigrams(content, true).into_iter().collect(),
            compressed: super::codec::compress(content)?,
            symbols: postcard::to_stdvec(symbols)
                .map_err(|e| Error::Codec(format!("symbols: {e}")))?,
            len: content.len() as u32,
        })
    }

    /// Uncompressed size.
    pub fn len(&self) -> usize {
        self.len as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn hash(&self) -> ObjectHash {
        self.hash
    }
}

/// Collects blobs and writes them out as one segment.
#[derive(Default)]
pub struct SegmentBuilder {
    docs: Vec<(ObjectHash, Vec<u8>, Vec<u8>, u32)>,
    postings: BTreeMap<u32, PostingList>,
    raw_bytes: usize,
}

impl SegmentBuilder {
    pub fn new() -> Self {
        SegmentBuilder::default()
    }

    /// Documents added so far.
    pub fn len(&self) -> usize {
        self.docs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    /// Uncompressed bytes added so far.
    pub fn raw_bytes(&self) -> usize {
        self.raw_bytes
    }

    /// Add one blob (already known to be text and within limits) with its
    /// symbols; returns its number in the segment.
    pub fn add(&mut self, hash: ObjectHash, content: &[u8], symbols: &[Symbol]) -> Result<u32> {
        Ok(self.push(PreparedDoc::new(hash, content, symbols)?))
    }

    /// Add a prepared blob; returns its number in the segment.
    pub fn push(&mut self, doc: PreparedDoc) -> u32 {
        let n = self.docs.len() as u32;
        for t in doc.trigrams {
            self.postings.entry(t).or_default().insert(n);
        }
        self.raw_bytes += doc.len as usize;
        self.docs
            .push((doc.hash, doc.compressed, doc.symbols, doc.len));
        n
    }

    /// The segment's bytes.
    pub fn build(self) -> Result<Bytes> {
        let mut dict = fst::MapBuilder::memory();
        let mut postings = Vec::new();
        for (t, list) in &self.postings {
            let offset = postings.len();
            list.serialize_into(&mut postings)
                .map_err(|e| Error::Codec(format!("posting list: {e}")))?;
            dict.insert(
                trigram_key(*t),
                pack_entry(offset, postings.len() - offset)?,
            )
            .map_err(|e| Error::Codec(format!("dictionary: {e}")))?;
        }
        let dict = dict
            .into_inner()
            .map_err(|e| Error::Codec(format!("dictionary: {e}")))?;

        let mut contents = Vec::new();
        let mut symbols = Vec::new();
        let mut docs = Vec::with_capacity(self.docs.len());
        for (hash, compressed, syms, len) in self.docs {
            docs.push(SegmentDoc {
                hash,
                len,
                content: Span {
                    offset: contents.len() as u64,
                    len: compressed.len() as u64,
                },
                symbols: Span {
                    offset: symbols.len() as u64,
                    len: syms.len() as u64,
                },
            });
            contents.extend_from_slice(&compressed);
            symbols.extend_from_slice(&syms);
        }

        let mut at = 0u64;
        let mut span = |len: usize| {
            let s = Span {
                offset: at,
                len: len as u64,
            };
            at += len as u64;
            s
        };
        let header = Header {
            docs,
            dict: span(dict.len()),
            postings: span(postings.len()),
            contents: span(contents.len()),
            symbols: span(symbols.len()),
        };
        let header = postcard::to_stdvec(&header).map_err(|e| Error::Codec(e.to_string()))?;
        let mut out = BytesMut::with_capacity(
            12 + header.len() + dict.len() + postings.len() + contents.len() + symbols.len(),
        );
        out.put_slice(MAGIC);
        out.put_u32_le(header.len() as u32);
        out.put_slice(&header);
        out.put_slice(&dict);
        out.put_slice(&postings);
        out.put_slice(&contents);
        out.put_slice(&symbols);
        Ok(out.freeze())
    }
}
