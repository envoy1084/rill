//! Posting lists: the sorted documents containing one trigram.
//!
//! Documents are dense `u32` ids assigned by the index's document table in
//! insertion order (each id names one blob hash). Gaps between consecutive
//! ids are small, so a varint-encoded delta list is a fraction of the size
//! of the raw ids and a tiny fraction of a list of 32-byte hashes.

use crate::error::{Error, Result};

/// Dense identifier of an indexed blob, assigned by the document table.
pub type DocId = u32;

/// Sorted, duplicate-free documents containing one trigram.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PostingList {
    docs: Vec<DocId>,
}

impl PostingList {
    /// The empty list.
    pub fn new() -> Self {
        PostingList::default()
    }

    /// Build from documents in any order; duplicates are dropped.
    pub fn from_docs(mut docs: Vec<DocId>) -> Self {
        docs.sort_unstable();
        docs.dedup();
        PostingList { docs }
    }

    /// Insert `doc`, keeping the list sorted. Ids usually arrive in
    /// increasing order, so this is an append in the common case.
    pub fn insert(&mut self, doc: DocId) {
        match self.docs.last() {
            Some(&last) if last < doc => self.docs.push(doc),
            Some(&last) if last == doc => {}
            _ => {
                if let Err(pos) = self.docs.binary_search(&doc) {
                    self.docs.insert(pos, doc);
                }
            }
        }
    }

    /// The documents, ascending.
    pub fn docs(&self) -> &[DocId] {
        &self.docs
    }

    /// Number of documents.
    pub fn len(&self) -> usize {
        self.docs.len()
    }

    /// True when no document contains the trigram.
    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    /// Varint-encoded deltas between consecutive ids.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.docs.len() * 2);
        let mut prev = 0u32;
        for &doc in &self.docs {
            write_varint(&mut out, doc - prev);
            prev = doc;
        }
        out
    }

    /// Inverse of [`PostingList::encode`].
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut docs = Vec::new();
        let mut prev = 0u32;
        let mut pos = 0;
        while pos < bytes.len() {
            let (delta, used) = read_varint(&bytes[pos..])?;
            pos += used;
            prev = prev
                .checked_add(delta)
                .ok_or_else(|| Error::Codec("posting delta overflow".into()))?;
            docs.push(prev);
        }
        Ok(PostingList { docs })
    }
}

fn write_varint(out: &mut Vec<u8>, mut value: u32) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn read_varint(bytes: &[u8]) -> Result<(u32, usize)> {
    let mut value = 0u32;
    for (i, &b) in bytes.iter().enumerate().take(5) {
        value |= u32::from(b & 0x7f) << (7 * i);
        if b & 0x80 == 0 {
            return Ok((value, i + 1));
        }
    }
    Err(Error::Codec("truncated or overlong varint".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_and_sorts() {
        let list = PostingList::from_docs(vec![300, 1, 1, 70000, 2]);
        assert_eq!(list.docs(), &[1, 2, 300, 70000]);
        let bytes = list.encode();
        assert!(bytes.len() < 4 * 4);
        assert_eq!(PostingList::decode(&bytes).unwrap(), list);
        assert_eq!(PostingList::decode(&[]).unwrap(), PostingList::new());
    }

    #[test]
    fn insert_keeps_order() {
        let mut list = PostingList::new();
        list.insert(5);
        list.insert(9);
        list.insert(9);
        list.insert(2);
        assert_eq!(list.docs(), &[2, 5, 9]);
    }

    #[test]
    fn rejects_truncated_varint() {
        assert!(PostingList::decode(&[0x80]).is_err());
    }
}
