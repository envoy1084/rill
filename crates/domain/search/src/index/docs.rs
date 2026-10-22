//! The document table: dense ids for indexed blob hashes.

use std::collections::HashMap;

use rill_types::ObjectHash;
use serde::{Deserialize, Serialize};

use crate::trigram::DocId;

/// Bidirectional map between blob hashes and posting-list document ids.
///
/// Ids are positions in `hashes`, assigned in insertion order and never
/// reused, so a persisted table and its posting lists stay consistent as
/// long as they are written together.
#[derive(Debug, Default, Clone)]
pub struct DocTable {
    hashes: Vec<ObjectHash>,
    ids: HashMap<ObjectHash, DocId>,
}

/// Persisted form of a [`DocTable`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocTableRecord {
    /// Hash of every document, indexed by id.
    pub hashes: Vec<ObjectHash>,
}

impl DocTable {
    /// An empty table.
    pub fn new() -> Self {
        DocTable::default()
    }

    /// Rebuild from a persisted record.
    pub fn from_record(record: DocTableRecord) -> Self {
        let ids = record
            .hashes
            .iter()
            .enumerate()
            .map(|(i, h)| (*h, i as DocId))
            .collect();
        DocTable {
            hashes: record.hashes,
            ids,
        }
    }

    /// Snapshot for persistence.
    pub fn to_record(&self) -> DocTableRecord {
        DocTableRecord {
            hashes: self.hashes.clone(),
        }
    }

    /// The id of `hash`, assigning the next free one if it is new.
    pub fn insert(&mut self, hash: ObjectHash) -> DocId {
        if let Some(&id) = self.ids.get(&hash) {
            return id;
        }
        let id = self.hashes.len() as DocId;
        self.hashes.push(hash);
        self.ids.insert(hash, id);
        id
    }

    /// The id of `hash`, if indexed.
    pub fn id_of(&self, hash: &ObjectHash) -> Option<DocId> {
        self.ids.get(hash).copied()
    }

    /// The hash behind `id`.
    pub fn hash_of(&self, id: DocId) -> Option<ObjectHash> {
        self.hashes.get(id as usize).copied()
    }

    /// Number of documents.
    pub fn len(&self) -> usize {
        self.hashes.len()
    }

    /// True when nothing has been indexed.
    pub fn is_empty(&self) -> bool {
        self.hashes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_types::ObjectKind;

    #[test]
    fn assigns_dense_stable_ids() {
        let a = ObjectHash::digest(ObjectKind::Blob, b"a");
        let b = ObjectHash::digest(ObjectKind::Blob, b"b");
        let mut t = DocTable::new();
        assert_eq!(t.insert(a), 0);
        assert_eq!(t.insert(b), 1);
        assert_eq!(t.insert(a), 0);
        let t2 = DocTable::from_record(t.to_record());
        assert_eq!(t2.id_of(&b), Some(1));
        assert_eq!(t2.hash_of(0), Some(a));
        assert_eq!(t2.len(), 2);
    }
}
