//! A SCIP index reduced to what lookups need: per file, each occurrence
//! of a global symbol, and each symbol's short name.

use std::collections::HashMap;

use protobuf::Message;
use rill_types::{ObjectHash, RepoPathBuf};
use scip::types::{Index, SymbolRole};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// One file's occurrences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScipDocument {
    pub path: RepoPathBuf,
    /// The content the index was built from; occurrences hold only while
    /// the file still has it.
    pub hash: ObjectHash,
    pub occurrences: Vec<ScipOccurrence>,
}

/// A symbol's use or definition in a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScipOccurrence {
    /// Index into [`ScipIndex::symbols`].
    pub symbol: u32,
    /// 1-based line.
    pub line: u32,
    /// 0-based columns (UTF-8 bytes as the indexer counts them).
    pub start: u32,
    pub end: u32,
    pub definition: bool,
}

/// A whole uploaded index.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScipIndex {
    /// Full SCIP symbol strings.
    pub symbols: Vec<String>,
    /// Each symbol's last descriptor name (`find_symbols`, `Foo`).
    pub names: Vec<String>,
    pub documents: Vec<ScipDocument>,
}

impl ScipIndex {
    /// Parse an encoded SCIP `Index`. Paths are relative to `prefix`
    /// (empty for the repository root); `hash_of` gives each file's
    /// current content, and files it does not know are dropped.
    pub fn parse(
        bytes: &[u8],
        prefix: &str,
        hash_of: impl Fn(&RepoPathBuf) -> Option<ObjectHash>,
    ) -> Result<ScipIndex> {
        let index =
            Index::parse_from_bytes(bytes).map_err(|e| Error::InvalidScip(e.to_string()))?;
        let mut out = ScipIndex::default();
        let mut ids: HashMap<String, u32> = HashMap::new();
        for doc in index.documents {
            let joined = match prefix.trim_matches('/') {
                "" => doc.relative_path.clone(),
                p => format!("{p}/{}", doc.relative_path),
            };
            let Ok(path) = RepoPathBuf::new(&joined) else {
                continue;
            };
            let Some(hash) = hash_of(&path) else {
                continue;
            };
            let mut occurrences = Vec::with_capacity(doc.occurrences.len());
            for occ in &doc.occurrences {
                if occ.symbol.is_empty() || scip::symbol::is_local_symbol(&occ.symbol) {
                    continue;
                }
                let Some((line, start, end)) = span(&occ.range) else {
                    continue;
                };
                let symbol = match ids.get(&occ.symbol) {
                    Some(&id) => id,
                    None => {
                        let id = out.symbols.len() as u32;
                        out.names.push(short_name(&occ.symbol));
                        out.symbols.push(occ.symbol.clone());
                        ids.insert(occ.symbol.clone(), id);
                        id
                    }
                };
                occurrences.push(ScipOccurrence {
                    symbol,
                    line,
                    start,
                    end,
                    definition: occ.symbol_roles & SymbolRole::Definition as i32 != 0,
                });
            }
            out.documents.push(ScipDocument {
                path,
                hash,
                occurrences,
            });
        }
        Ok(out)
    }
}

/// `[line, start, end]` or `[line, start, end_line, end]`, 0-based, as a
/// 1-based line and the columns on it.
fn span(range: &[i32]) -> Option<(u32, u32, u32)> {
    let (line, start, end) = match *range {
        [l, s, e] => (l, s, e),
        [l, s, el, e] if el == l => (l, s, e),
        [l, s, _, _] => (l, s, s),
        _ => return None,
    };
    (line >= 0 && start >= 0 && end >= 0).then(|| (line as u32 + 1, start as u32, end as u32))
}

fn short_name(symbol: &str) -> String {
    scip::symbol::parse_symbol(symbol)
        .ok()
        .and_then(|s| s.descriptors.last().map(|d| d.name.clone()))
        .unwrap_or_default()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use scip::types::{Document, Occurrence};

    /// A symbol, its `[line, start, end]` and whether it is a definition.
    pub(crate) type Occ<'a> = (&'a str, [i32; 3], bool);

    pub(crate) fn encoded(docs: &[(&str, &[Occ<'_>])]) -> Vec<u8> {
        let mut index = Index::new();
        for (path, occs) in docs {
            let mut doc = Document::new();
            doc.relative_path = path.to_string();
            for (symbol, range, def) in occs.iter() {
                let mut o = Occurrence::new();
                o.symbol = symbol.to_string();
                o.range = range.to_vec();
                o.symbol_roles = if *def {
                    SymbolRole::Definition as i32
                } else {
                    0
                };
                doc.occurrences.push(o);
            }
            index.documents.push(doc);
        }
        index.write_to_bytes().unwrap()
    }

    pub(crate) const GREET: &str = "rust-analyzer cargo demo 0.1.0 lib/greet().";

    #[test]
    fn keeps_global_occurrences_of_known_files() {
        let bytes = encoded(&[
            (
                "src/lib.rs",
                &[(GREET, [0, 7, 12], true), ("local 3", [1, 0, 1], false)],
            ),
            ("src/main.rs", &[(GREET, [4, 4, 9], false)]),
            ("gone.rs", &[(GREET, [0, 0, 5], false)]),
        ]);
        let known = |p: &RepoPathBuf| {
            (p.as_str() != "sub/gone.rs")
                .then(|| ObjectHash::digest(rill_types::ObjectKind::Blob, p.as_str().as_bytes()))
        };
        let index = ScipIndex::parse(&bytes, "sub", known).unwrap();
        assert_eq!(index.names, vec!["greet"]);
        assert_eq!(index.documents.len(), 2);
        let lib = &index.documents[0];
        assert_eq!(lib.path.as_str(), "sub/src/lib.rs");
        assert_eq!(lib.occurrences.len(), 1, "locals are dropped");
        assert_eq!(
            (lib.occurrences[0].line, lib.occurrences[0].definition),
            (1, true)
        );
        assert!(ScipIndex::parse(b"\xff\xff", "", known).is_err());
    }
}
