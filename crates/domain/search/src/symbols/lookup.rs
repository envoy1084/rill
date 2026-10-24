//! Looking symbols up in a scope: definitions by name, symbols whose
//! names contain a string, and one file's symbols. The trigram index
//! narrows the blobs; each candidate's stored symbols are then checked.

use std::collections::HashSet;
use std::sync::Arc;

use futures::stream::{self, StreamExt};
use rill_core::read_content;
use rill_types::{ObjectKind, RepoPathBuf};

use super::extract::{extract_symbols, Symbol, SymbolKind};
use crate::error::Result;
use crate::index::SearchIndex;
use crate::query::{rank, PathFilter, Scope};
use crate::trigram::{Candidates, TrigramQuery};

/// Which names a symbol lookup wants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolName {
    /// Exactly this name (case-sensitive).
    Exact(String),
    /// Names containing this (case-insensitive).
    Contains(String),
}

impl SymbolName {
    fn text(&self) -> &str {
        match self {
            SymbolName::Exact(s) | SymbolName::Contains(s) => s,
        }
    }

    fn matches(&self, name: &str) -> bool {
        match self {
            SymbolName::Exact(s) => name == s,
            SymbolName::Contains(s) => name.to_lowercase().contains(&s.to_lowercase()),
        }
    }
}

/// A symbol lookup.
#[derive(Debug, Clone)]
pub struct SymbolQuery {
    pub name: SymbolName,
    /// Only these kinds; empty is every kind.
    pub kinds: Vec<SymbolKind>,
    pub limit: usize,
}

/// A symbol and the file defining it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolHit {
    pub path: RepoPathBuf,
    pub symbol: Symbol,
}

impl SymbolQuery {
    fn wants(&self, s: &Symbol) -> bool {
        (self.kinds.is_empty() || self.kinds.contains(&s.kind)) && self.name.matches(&s.name)
    }
}

/// Symbols `query` names in `scope`, at paths `allow` admits; source
/// files over test and vendored code, then by path and line.
pub async fn find_symbols<F: PathFilter + Clone + 'static>(
    index: &SearchIndex,
    scope: &Scope,
    query: &SymbolQuery,
    allow: F,
) -> Result<Vec<SymbolHit>> {
    index.refresh().await?;
    let plan = TrigramQuery::literal(query.name.text().as_bytes());
    let shadowed: Arc<HashSet<RepoPathBuf>> =
        Arc::new(scope.live.iter().map(|f| f.path.clone()).collect());
    let mut segments = Vec::new();
    for shard in 0..index.config().shards.max(1) as usize {
        segments.push(index.segments_of(shard).await?);
    }
    let (view, wanted, objects) = (
        scope.view.clone(),
        query.clone(),
        index.repo().objects().clone(),
    );
    let (admit, shadow) = (allow.clone(), shadowed.clone());
    let per_shard = crate::cpu::map(segments, move |segments| -> Result<Vec<SymbolHit>> {
        let mut out = Vec::new();
        for seg in &segments {
            let candidates = seg.candidates(&plan)?;
            let docs: Box<dyn Iterator<Item = u32>> = match &candidates {
                Candidates::All => Box::new(0..seg.docs().len() as u32),
                Candidates::Docs(d) => Box::new(d.iter()),
            };
            for doc in docs {
                let hash = seg.docs()[doc as usize].hash;
                if !view.holds(&hash) || objects.refuses(ObjectKind::Blob, &hash).is_err() {
                    continue;
                }
                let symbols: Vec<Symbol> = seg
                    .symbols(doc)?
                    .into_iter()
                    .filter(|s| wanted.wants(s))
                    .collect();
                if symbols.is_empty() {
                    continue;
                }
                for path in view.paths_of(&hash) {
                    if shadow.contains(&path) || !admit(&path) {
                        continue;
                    }
                    out.extend(symbols.iter().map(|s| SymbolHit {
                        path: path.clone(),
                        symbol: s.clone(),
                    }));
                }
                if out.len() >= wanted.limit {
                    return Ok(out);
                }
            }
        }
        Ok(out)
    })
    .await?;
    let mut hits: Vec<SymbolHit> = per_shard.into_iter().flatten().collect();
    let live: Vec<_> = scope
        .live
        .iter()
        .filter(|f| allow(&f.path))
        .cloned()
        .collect();
    let objects = index.repo().objects().clone();
    let mut reads = stream::iter(live)
        .map(|f| {
            let objects = objects.clone();
            let query = query.clone();
            async move {
                let bytes = match read_content(objects.as_ref(), &f.content).await {
                    Err(e) if e.is_redacted() => return Ok::<_, crate::Error>(Vec::new()),
                    other => other?,
                };
                let path = f.path.clone();
                let symbols =
                    tokio::task::spawn_blocking(move || extract_symbols(path.as_str(), &bytes))
                        .await?;
                Ok(symbols
                    .into_iter()
                    .filter(|s| query.wants(s))
                    .map(|symbol| SymbolHit {
                        path: f.path.clone(),
                        symbol,
                    })
                    .collect())
            }
        })
        .buffered(16);
    while let Some(found) = reads.next().await {
        hits.extend(found?);
    }
    let order = |h: &SymbolHit| {
        let s = rank::score(&rank::Signals {
            path: &h.path,
            name_matches: false,
            defines: true,
            recent: false,
        });
        (std::cmp::Reverse(s), h.path.clone(), h.symbol.line)
    };
    hits.sort_by_cached_key(order);
    hits.dedup();
    hits.truncate(query.limit);
    Ok(hits)
}

/// The symbols of the file at `path` in `scope`; `None` when there is no
/// such file.
pub async fn file_symbols(
    index: &SearchIndex,
    scope: &Scope,
    path: &RepoPathBuf,
) -> Result<Option<Vec<Symbol>>> {
    if let Some(f) = scope.live.iter().find(|f| &f.path == path) {
        let bytes = read_content(index.repo().objects().as_ref(), &f.content).await?;
        let path = path.clone();
        let symbols =
            tokio::task::spawn_blocking(move || extract_symbols(path.as_str(), &bytes)).await?;
        return Ok(Some(symbols));
    }
    let Some((hash, _)) = scope.view.get(path) else {
        return Ok(None);
    };
    index.refresh().await?;
    index.repo().objects().refuses(ObjectKind::Blob, &hash)?;
    for seg in index.segments_of(index.shard_of(&hash)).await? {
        if let Some(doc) = seg.doc_of(&hash) {
            return Ok(Some(seg.symbols(doc)?));
        }
    }
    Ok(Some(Vec::new()))
}
