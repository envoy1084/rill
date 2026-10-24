//! Storing uploaded SCIP indexes and finding the one for a root.
//!
//! * The reduced index is an object (`ObjectKind::Derived`, postcard then
//!   zstd).
//! * `idx:scip@1` by root tree names it; under the zero key, the latest
//!   upload. A root with no upload of its own uses the latest, trusted
//!   file by file where the content is unchanged.

use std::collections::HashMap;
use std::sync::Arc;

use bytes::Bytes;
use rill_store::repo::{decode_tagged, encode_tagged};
use rill_types::{ObjectHash, ObjectKind, RepoPathBuf, TreeHash};
use serde::{Deserialize, Serialize};

use super::index::{ScipIndex, ScipOccurrence};
use crate::error::{Error, Result};
use crate::index::SearchIndex;
use crate::paths::PathView;

const KIND_SCIP: &str = "idx:scip@1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct ScipRef {
    root: TreeHash,
    object: ObjectHash,
}

/// A loaded index with its lookup tables.
pub(crate) struct LoadedScip {
    index: ScipIndex,
    /// Symbol id to (document, occurrence) positions.
    by_symbol: HashMap<u32, Vec<(u32, u32)>>,
    by_name: HashMap<String, Vec<u32>>,
}

impl LoadedScip {
    fn new(index: ScipIndex) -> LoadedScip {
        let mut by_symbol: HashMap<u32, Vec<(u32, u32)>> = HashMap::new();
        for (d, doc) in index.documents.iter().enumerate() {
            for (o, occ) in doc.occurrences.iter().enumerate() {
                by_symbol
                    .entry(occ.symbol)
                    .or_default()
                    .push((d as u32, o as u32));
            }
        }
        let mut by_name: HashMap<String, Vec<u32>> = HashMap::new();
        for (id, name) in index.names.iter().enumerate() {
            by_name.entry(name.clone()).or_default().push(id as u32);
        }
        LoadedScip {
            index,
            by_symbol,
            by_name,
        }
    }
}

/// A file position a SCIP index reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScipLocation {
    pub path: RepoPathBuf,
    pub occurrence: ScipOccurrence,
    /// The full SCIP symbol, and its short name.
    pub symbol: String,
    pub name: String,
}

/// The SCIP index that applies to one scope.
pub struct ScipView {
    loaded: Arc<LoadedScip>,
    view: Arc<PathView>,
    /// Uploaded for this very root (else the latest upload).
    pub exact: bool,
}

impl ScipView {
    fn holds(&self, doc: u32) -> bool {
        let doc = &self.loaded.index.documents[doc as usize];
        self.view.get(&doc.path).is_some_and(|(h, _)| h == doc.hash)
    }

    fn located(&self, symbol: u32, keep: impl Fn(&ScipOccurrence) -> bool) -> Vec<ScipLocation> {
        let mut out = Vec::new();
        for &(d, o) in self.loaded.by_symbol.get(&symbol).into_iter().flatten() {
            let doc = &self.loaded.index.documents[d as usize];
            let occ = doc.occurrences[o as usize];
            if keep(&occ) && self.holds(d) {
                out.push(ScipLocation {
                    path: doc.path.clone(),
                    occurrence: occ,
                    symbol: self.loaded.index.symbols[symbol as usize].clone(),
                    name: self.loaded.index.names[symbol as usize].clone(),
                });
            }
        }
        out
    }

    fn named(&self, name: &str) -> &[u32] {
        self.loaded.by_name.get(name).map_or(&[], Vec::as_slice)
    }

    /// Where symbols with this short name are defined.
    pub fn definitions(&self, name: &str) -> Vec<ScipLocation> {
        self.named(name)
            .iter()
            .flat_map(|&s| self.located(s, |o| o.definition))
            .collect()
    }

    /// Every occurrence of symbols with this short name.
    pub fn references(&self, name: &str) -> Vec<ScipLocation> {
        self.named(name)
            .iter()
            .flat_map(|&s| self.located(s, |_| true))
            .collect()
    }

    /// The symbol at a 1-based line and 0-based column of `path`.
    pub fn symbol_at(&self, path: &RepoPathBuf, line: u32, column: u32) -> Option<String> {
        let (d, doc) = self
            .loaded
            .index
            .documents
            .iter()
            .enumerate()
            .find(|(_, doc)| &doc.path == path)?;
        if !self.holds(d as u32) {
            return None;
        }
        doc.occurrences
            .iter()
            .find(|o| o.line == line && o.start <= column && column <= o.end)
            .map(|o| self.loaded.index.symbols[o.symbol as usize].clone())
    }

    /// Every occurrence of the full SCIP `symbol`.
    pub fn occurrences(&self, symbol: &str) -> Vec<ScipLocation> {
        match self.loaded.index.symbols.iter().position(|s| s == symbol) {
            Some(id) => self.located(id as u32, |_| true),
            None => Vec::new(),
        }
    }
}

/// Recently loaded indexes, by object.
#[derive(Default)]
pub(crate) struct ScipCache(parking_lot::Mutex<Vec<(ObjectHash, Arc<LoadedScip>)>>);

const CACHED: usize = 4;

/// Store an encoded SCIP index for `root`, whose files are `view`; paths
/// in it are relative to `prefix`. Returns the files and occurrences
/// kept.
pub async fn store_scip(
    index: &SearchIndex,
    root: TreeHash,
    view: Arc<PathView>,
    bytes: Bytes,
    prefix: String,
) -> Result<(usize, usize)> {
    let (parsed, encoded) = tokio::task::spawn_blocking(move || -> Result<_> {
        let parsed = ScipIndex::parse(&bytes, &prefix, |p| view.get(p).map(|(h, _)| h))?;
        let raw = postcard::to_allocvec(&parsed).map_err(|e| Error::Codec(e.to_string()))?;
        let encoded =
            zstd::encode_all(raw.as_slice(), 3).map_err(|e| Error::Codec(e.to_string()))?;
        Ok((parsed, Bytes::from(encoded)))
    })
    .await??;
    let object = ObjectHash::digest(ObjectKind::Derived, &encoded);
    let repo = index.repo();
    repo.objects()
        .put(ObjectKind::Derived, &object, encoded)
        .await?;
    let r = encode_tagged(KIND_SCIP, &ScipRef { root, object })?;
    repo.put_derived(KIND_SCIP, &root, r.clone()).await?;
    repo.put_derived(KIND_SCIP, &ObjectHash::ZERO, r).await?;
    let occurrences = parsed.documents.iter().map(|d| d.occurrences.len()).sum();
    let files = parsed.documents.len();
    index
        .scip_cache()
        .insert(object, Arc::new(LoadedScip::new(parsed)));
    Ok((files, occurrences))
}

/// The SCIP index for `root` (its own upload, else the latest), over the
/// scope's `view`; `None` when nothing was uploaded.
pub async fn scip_for(
    index: &SearchIndex,
    root: TreeHash,
    view: Arc<PathView>,
) -> Result<Option<ScipView>> {
    let repo = index.repo();
    let (r, exact) = match repo.get_derived(KIND_SCIP, &root).await? {
        Some(b) => (decode_tagged::<ScipRef>(KIND_SCIP, &b)?, true),
        None => match repo.get_derived(KIND_SCIP, &ObjectHash::ZERO).await? {
            Some(b) => (decode_tagged::<ScipRef>(KIND_SCIP, &b)?, false),
            None => return Ok(None),
        },
    };
    let loaded = match index.scip_cache().get(&r.object) {
        Some(l) => l,
        None => {
            let bytes = repo
                .objects()
                .get(ObjectKind::Derived, &r.object)
                .await?
                .ok_or_else(|| Error::Codec(format!("SCIP index {} is missing", r.object)))?;
            let loaded = tokio::task::spawn_blocking(move || -> Result<LoadedScip> {
                let raw =
                    zstd::decode_all(bytes.as_ref()).map_err(|e| Error::Codec(e.to_string()))?;
                let index: ScipIndex =
                    postcard::from_bytes(&raw).map_err(|e| Error::Codec(e.to_string()))?;
                Ok(LoadedScip::new(index))
            })
            .await??;
            let loaded = Arc::new(loaded);
            index.scip_cache().insert(r.object, loaded.clone());
            loaded
        }
    };
    Ok(Some(ScipView {
        loaded,
        view,
        exact: exact || r.root == root,
    }))
}

impl ScipCache {
    fn get(&self, object: &ObjectHash) -> Option<Arc<LoadedScip>> {
        self.0
            .lock()
            .iter()
            .find(|(h, _)| h == object)
            .map(|(_, l)| l.clone())
    }

    fn insert(&self, object: ObjectHash, loaded: Arc<LoadedScip>) {
        let mut cache = self.0.lock();
        cache.retain(|(h, _)| *h != object);
        cache.insert(0, (object, loaded));
        cache.truncate(CACHED);
    }
}

#[cfg(test)]
mod tests {
    use super::super::index::tests::{encoded, GREET};
    use super::*;

    #[test]
    fn occurrences_hold_only_for_unchanged_files() {
        let bytes = encoded(&[
            ("lib.rs", &[(GREET, [0, 7, 12], true)]),
            ("main.rs", &[(GREET, [4, 4, 9], false)]),
        ]);
        let hash = |s: &str| ObjectHash::digest(ObjectKind::Blob, s.as_bytes());
        let parsed = ScipIndex::parse(&bytes, "", |p| Some(hash(p.as_str()))).unwrap();
        let entry = |p: &str, h| crate::paths::PathEntry {
            path: RepoPathBuf::new(p).unwrap(),
            hash: h,
            kind: rill_types::FileKind::Regular,
        };
        let view = PathView::base(vec![
            entry("lib.rs", hash("lib.rs")),
            entry("main.rs", hash("edited")),
        ]);
        let v = ScipView {
            loaded: Arc::new(LoadedScip::new(parsed)),
            view: Arc::new(view),
            exact: false,
        };
        assert_eq!(v.definitions("greet").len(), 1);
        let refs = v.references("greet");
        assert_eq!(refs.len(), 1, "main.rs changed since the upload");
        assert_eq!(refs[0].path.as_str(), "lib.rs");
        let lib = RepoPathBuf::new("lib.rs").unwrap();
        assert_eq!(v.symbol_at(&lib, 1, 9).as_deref(), Some(GREET));
        assert_eq!(v.symbol_at(&lib, 2, 9), None);
    }
}
