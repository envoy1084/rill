//! Per-root symbol tables.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use bytes::Bytes;
use dashmap::DashMap;
use futures::stream::{self, StreamExt, TryStreamExt};
use rill_core::{read_content, TreeOps};
use rill_store::repo::{decode_tagged, encode_tagged};
use rill_store::RepoStore;
use rill_types::{ObjectHash, RepoPath, RepoPathBuf, SparseProfile, TreeHash};
use serde::{Deserialize, Serialize};

use super::extract::{extract_for, Symbol};
use super::lang::Language;
use crate::error::Result;

/// Derived kind under which a root's symbol table is stored.
pub const KIND_SYMBOLS: &str = "idx:sym@1";

/// Largest file the extractor will read.
const MAX_SYMBOL_FILE: usize = 1024 * 1024;

/// A symbol together with the file that defines it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolLocation {
    pub path: RepoPathBuf,
    pub symbol: Symbol,
}

/// Persisted symbol table of one root.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SymbolsRecord {
    by_path: BTreeMap<RepoPathBuf, Vec<Symbol>>,
}

/// Symbols of one root, indexed by path and by name.
#[derive(Debug, Default)]
pub struct RootSymbols {
    by_path: BTreeMap<RepoPathBuf, Vec<Symbol>>,
    by_name: HashMap<String, Vec<(RepoPathBuf, usize)>>,
}

impl RootSymbols {
    fn from_record(record: SymbolsRecord) -> Self {
        let mut by_name: HashMap<String, Vec<(RepoPathBuf, usize)>> = HashMap::new();
        for (path, symbols) in &record.by_path {
            for (i, s) in symbols.iter().enumerate() {
                by_name
                    .entry(s.name.clone())
                    .or_default()
                    .push((path.clone(), i));
            }
        }
        RootSymbols {
            by_path: record.by_path,
            by_name,
        }
    }

    /// Every definition named exactly `name`, in path order.
    pub fn find_definition(&self, name: &str) -> Vec<SymbolLocation> {
        self.by_name
            .get(name)
            .map(|locs| {
                locs.iter()
                    .map(|(path, i)| SymbolLocation {
                        path: path.clone(),
                        symbol: self.by_path[path][*i].clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Definitions in `path`, in line order.
    pub fn list_symbols(&self, path: &RepoPath) -> &[Symbol] {
        self.by_path.get(path).map_or(&[], Vec::as_slice)
    }

    /// Number of files with at least one symbol.
    pub fn file_count(&self) -> usize {
        self.by_path.len()
    }
}

/// Builds and caches symbol tables per root tree.
///
/// Extraction is cached per `(content hash, language)` because the same
/// blob recurs across roots; the per-root table itself is persisted as
/// derived data so a root is walked once.
pub struct SymbolIndex {
    repo: RepoStore,
    trees: TreeOps,
    roots: DashMap<TreeHash, Arc<RootSymbols>>,
    blobs: DashMap<(ObjectHash, Language), Arc<Vec<Symbol>>>,
}

impl SymbolIndex {
    /// A symbol index over `repo`.
    pub fn new(repo: RepoStore, trees: TreeOps) -> Arc<Self> {
        Arc::new(SymbolIndex {
            repo,
            trees,
            roots: DashMap::new(),
            blobs: DashMap::new(),
        })
    }

    /// The symbol table of `root`, building and storing it if needed.
    pub async fn for_root(&self, root: TreeHash) -> Result<Arc<RootSymbols>> {
        if let Some(r) = self.roots.get(&root) {
            return Ok(r.clone());
        }
        if let Some(bytes) = self.repo.get_derived(KIND_SYMBOLS, &root).await? {
            let record: SymbolsRecord = decode_tagged(KIND_SYMBOLS, &bytes)?;
            let built = Arc::new(RootSymbols::from_record(record));
            self.roots.insert(root, built.clone());
            return Ok(built);
        }
        let record = self.build(root).await?;
        self.repo
            .put_derived(KIND_SYMBOLS, &root, encode_tagged(KIND_SYMBOLS, &record)?)
            .await?;
        let built = Arc::new(RootSymbols::from_record(record));
        self.roots.insert(root, built.clone());
        Ok(built)
    }

    /// Definitions of `name` under `root`.
    pub async fn find_definition(&self, root: TreeHash, name: &str) -> Result<Vec<SymbolLocation>> {
        Ok(self.for_root(root).await?.find_definition(name))
    }

    /// Definitions in `path` under `root`.
    pub async fn list_symbols(&self, root: TreeHash, path: &RepoPath) -> Result<Vec<Symbol>> {
        Ok(self.for_root(root).await?.list_symbols(path).to_vec())
    }

    async fn build(&self, root: TreeHash) -> Result<SymbolsRecord> {
        let entries = self
            .trees
            .walk(root, &RepoPathBuf::root(), &SparseProfile::full())
            .await?;
        let objects = self.trees.objects().clone();
        let extracted: Vec<(RepoPathBuf, Arc<Vec<Symbol>>)> = stream::iter(entries)
            .filter_map(|e| async move {
                Language::from_path(e.path.as_str())
                    .filter(|_| e.content.size() as usize <= MAX_SYMBOL_FILE)
                    .map(|lang| (e, lang))
            })
            .map(|(e, lang)| {
                let objects = objects.clone();
                async move {
                    let key = (e.content.content_hash(), lang);
                    let symbols = match self.blobs.get(&key) {
                        Some(cached) => cached.clone(),
                        None => {
                            let bytes: Bytes = read_content(objects.as_ref(), &e.content).await?;
                            let symbols =
                                tokio::task::spawn_blocking(move || extract_for(lang, &bytes))
                                    .await?;
                            let symbols = Arc::new(symbols);
                            self.blobs.insert(key, symbols.clone());
                            symbols
                        }
                    };
                    Ok::<_, crate::error::Error>((e.path, symbols))
                }
            })
            .buffer_unordered(16)
            .try_collect()
            .await?;
        let mut record = SymbolsRecord::default();
        for (path, symbols) in extracted {
            if !symbols.is_empty() {
                record.by_path.insert(path, (*symbols).clone());
            }
        }
        tracing::info!(root = %root, files = record.by_path.len(), "built symbol table");
        Ok(record)
    }
}
