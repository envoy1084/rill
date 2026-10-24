//! Code intelligence over a revision: symbol lists, definitions and
//! references (precise from an uploaded SCIP index where one covers the
//! file, else from extracted symbols and whole-word text matches), and
//! SCIP uploads.

use std::collections::HashSet;

use rill_proto::rill::v1 as pb;
use rill_search::{
    file_symbols, find_symbols, scip_for, store_scip, ScipLocation, Scope, SearchQuery,
    ShardSelect, SymbolHit, SymbolKind, SymbolName, SymbolQuery,
};
use rill_types::{ContentRef, EntryKind, RepoPathBuf};

use super::ContentSvc;
use crate::services::common::{limit, open_repo, path, Opened};
use crate::services::{revision, Call};
use rill_runtime::error::{Error, Result};

const MAX_SYMBOLS: usize = 1_000;
const MAX_DEFINITIONS: usize = 100;
const MAX_REFERENCES: usize = 1_000;

/// A revision's search scope, for the symbol RPCs.
struct Resolved {
    root: rill_types::TreeHash,
    scope: Scope,
}

impl ContentSvc {
    async fn symbol_scope(&self, opened: &Opened, revision: &str) -> Result<Resolved> {
        let r = revision::resolve(&opened.repo, revision).await?;
        let scope = opened.repo.search_scope(r.version, r.root, None).await?;
        Ok(Resolved {
            root: r.root,
            scope,
        })
    }

    pub(super) async fn list_symbols_impl(
        &self,
        request: Call<pb::ListSymbolsRequest>,
    ) -> Result<pb::ListSymbolsResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.repo).await?;
        let max = limit(req.max_results, MAX_SYMBOLS, MAX_SYMBOLS);
        let kinds = kinds(&req.kinds)?;
        let wanted = |k: SymbolKind| kinds.is_empty() || kinds.contains(&k);
        if !req.path.is_empty() {
            let file = path("path", &req.path)?;
            opened.permit().read(&file).await?;
            let resolved = self.symbol_scope(&opened, &req.revision).await?;
            let index = &opened.repo.search;
            let symbols = file_symbols(index, &resolved.scope, &file)
                .await?
                .ok_or_else(|| Error::not_found(format!("file {file}")))?;
            return Ok(pb::ListSymbolsResponse {
                symbols: symbols
                    .into_iter()
                    .filter(|s| wanted(s.kind))
                    .take(max)
                    .map(|symbol| {
                        defined(SymbolHit {
                            path: file.clone(),
                            symbol,
                        })
                    })
                    .collect(),
            });
        }
        if req.query.is_empty() {
            return Err(Error::invalid("give a path or a query"));
        }
        let filter = opened.permit().read_any().await?;
        let resolved = self.symbol_scope(&opened, &req.revision).await?;
        let query = SymbolQuery {
            name: SymbolName::Contains(req.query.clone()),
            kinds,
            limit: max,
        };
        let hits = find_symbols(&opened.repo.search, &resolved.scope, &query, move |p| {
            filter.allows(p)
        })
        .await?;
        Ok(pb::ListSymbolsResponse {
            symbols: hits.into_iter().map(defined).collect(),
        })
    }

    pub(super) async fn find_definitions_impl(
        &self,
        request: Call<pb::FindDefinitionsRequest>,
    ) -> Result<pb::FindDefinitionsResponse> {
        let req = request.get_ref();
        if req.name.is_empty() {
            return Err(Error::invalid("name is required"));
        }
        let opened = open_repo(&self.state, &request, &req.repo).await?;
        let filter = opened.permit().read_any().await?;
        let max = limit(req.max_results, MAX_DEFINITIONS, MAX_DEFINITIONS);
        let resolved = self.symbol_scope(&opened, &req.revision).await?;
        let index = &opened.repo.search;
        let mut out = Vec::new();
        let mut precise_files = HashSet::new();
        if let Some(scip) = scip_for(index, resolved.root, resolved.scope.view.clone()).await? {
            for loc in scip.definitions(&req.name) {
                if filter.allows(&loc.path) {
                    precise_files.insert(loc.path.clone());
                    out.push(located(loc));
                }
            }
        }
        let query = SymbolQuery {
            name: SymbolName::Exact(req.name.clone()),
            kinds: Vec::new(),
            limit: max,
        };
        let f = filter.clone();
        for hit in find_symbols(index, &resolved.scope, &query, move |p| f.allows(p)).await? {
            if !precise_files.contains(&hit.path) {
                out.push(defined(hit));
            }
        }
        out.truncate(max);
        Ok(pb::FindDefinitionsResponse { definitions: out })
    }

    pub(super) async fn find_references_impl(
        &self,
        request: Call<pb::FindReferencesRequest>,
    ) -> Result<pb::FindReferencesResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.repo).await?;
        let filter = opened.permit().read_any().await?;
        let max = limit(req.max_results, MAX_REFERENCES, MAX_REFERENCES);
        let resolved = self.symbol_scope(&opened, &req.revision).await?;
        let index = &opened.repo.search;
        let scip = scip_for(index, resolved.root, resolved.scope.view.clone()).await?;

        let at = if req.path.is_empty() {
            None
        } else {
            let file = path("path", &req.path)?;
            opened.permit().read(&file).await?;
            Some(file)
        };
        if let (Some(scip), Some(file)) = (&scip, &at) {
            if let Some(symbol) = scip.symbol_at(file, req.line, req.column) {
                return Ok(precise(scip.occurrences(&symbol), &filter, max));
            }
        }
        let name = match &at {
            None if req.name.is_empty() => return Err(Error::invalid("give a name or a position")),
            None => req.name.clone(),
            Some(file) => identifier_at(&opened, resolved.root, file, req.line, req.column).await?,
        };
        if let Some(scip) = &scip {
            let found = scip.references(&name);
            if !found.is_empty() {
                return Ok(precise(found, &filter, max));
            }
        }
        textual(&opened, resolved.scope, &name, filter, max).await
    }

    pub(super) async fn upload_scip_impl(
        &self,
        request: Call<pb::UploadScipRequest>,
    ) -> Result<pb::UploadScipResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.repo).await?;
        opened.permit().write_any().await?;
        let content = ContentRef::try_from(
            req.index
                .as_option()
                .cloned()
                .ok_or_else(|| Error::invalid("index is required"))?,
        )?;
        let resolved = self.symbol_scope(&opened, &req.revision).await?;
        let bytes = rill_core::read_content(opened.repo.repo.objects().as_ref(), &content).await?;
        let (files, occurrences) = store_scip(
            &opened.repo.search,
            resolved.root,
            resolved.scope.view.clone(),
            bytes,
            req.prefix.clone(),
        )
        .await?;
        tracing::info!(repo = %opened.repo.config.name, files, occurrences, "SCIP index stored");
        Ok(pb::UploadScipResponse {
            files: files as u32,
            occurrences: occurrences as u64,
        })
    }
}

fn kinds(names: &[String]) -> Result<Vec<SymbolKind>> {
    names
        .iter()
        .map(|n| {
            SymbolKind::parse(n).ok_or_else(|| Error::invalid(format!("unknown symbol kind {n:?}")))
        })
        .collect()
}

fn defined(hit: SymbolHit) -> pb::SymbolLocation {
    pb::SymbolLocation {
        path: hit.path.to_string(),
        line: hit.symbol.line,
        kind: hit.symbol.kind.as_str().into(),
        name: hit.symbol.name,
        definition: true,
        ..Default::default()
    }
}

fn located(loc: ScipLocation) -> pb::SymbolLocation {
    pb::SymbolLocation {
        path: loc.path.to_string(),
        line: loc.occurrence.line,
        name: loc.name,
        start_column: loc.occurrence.start,
        end_column: loc.occurrence.end,
        precise: true,
        definition: loc.occurrence.definition,
        symbol: loc.symbol,
        ..Default::default()
    }
}

fn precise(
    found: Vec<ScipLocation>,
    filter: &rill_policy::PathFilter,
    max: usize,
) -> pb::FindReferencesResponse {
    let mut refs: Vec<_> = found
        .into_iter()
        .filter(|l| filter.allows(&l.path))
        .map(located)
        .collect();
    refs.sort_by(|a, b| (&a.path, a.line, a.start_column).cmp(&(&b.path, b.line, b.start_column)));
    let truncated = refs.len() > max;
    refs.truncate(max);
    pb::FindReferencesResponse {
        references: refs,
        precise: true,
        truncated,
    }
}

/// Whole-word matches of `name`, with its definitions marked.
async fn textual(
    opened: &Opened,
    scope: Scope,
    name: &str,
    filter: rill_policy::PathFilter,
    max: usize,
) -> Result<pb::FindReferencesResponse> {
    let index = &opened.repo.search;
    let query = SymbolQuery {
        name: SymbolName::Exact(name.into()),
        kinds: Vec::new(),
        limit: MAX_DEFINITIONS,
    };
    let f = filter.clone();
    let defs: HashSet<(RepoPathBuf, u32)> =
        find_symbols(index, &scope, &query, move |p| f.allows(p))
            .await?
            .into_iter()
            .map(|h| (h.path, h.symbol.line))
            .collect();
    let search = SearchQuery::whole_word(name).max_results(max + 1);
    let found = opened
        .repo
        .grep_shards(scope, search, filter, ShardSelect::all())
        .await?;
    let result = rill_search::assemble(found.files, max + 1, found.capped, found.partial);
    let truncated = result.truncated || result.hits.len() > max;
    let references = result
        .hits
        .into_iter()
        .take(max)
        .map(|h| {
            let start = word_column(&h.line, name).unwrap_or(0) as u32;
            pb::SymbolLocation {
                definition: defs.contains(&(h.path.clone(), h.line_no)),
                path: h.path.to_string(),
                line: h.line_no,
                name: name.into(),
                start_column: start,
                end_column: start + name.len() as u32,
                ..Default::default()
            }
        })
        .collect();
    Ok(pb::FindReferencesResponse {
        references,
        precise: false,
        truncated,
    })
}

/// The identifier at a 1-based line and 0-based byte column of a file.
async fn identifier_at(
    opened: &Opened,
    root: rill_types::TreeHash,
    file: &RepoPathBuf,
    line: u32,
    column: u32,
) -> Result<String> {
    let Some(EntryKind::File { content, .. }) = opened.repo.trees.lookup(root, file).await? else {
        return Err(Error::not_found(format!("file {file}")));
    };
    let bytes = rill_core::read_content(opened.repo.repo.objects().as_ref(), &content).await?;
    let text = String::from_utf8_lossy(&bytes);
    let at = text
        .lines()
        .nth(line.saturating_sub(1) as usize)
        .ok_or_else(|| Error::invalid(format!("{file} has no line {line}")))?;
    let column = (column as usize).min(at.len());
    if !at.is_char_boundary(column) {
        return Err(Error::invalid(format!(
            "column {column} splits a character"
        )));
    }
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let start = at[..column]
        .rfind(|c: char| !word(c))
        .map_or(0, |i| i + at[i..].chars().next().map_or(1, char::len_utf8));
    let end = at[column..]
        .find(|c: char| !word(c))
        .map_or(at.len(), |i| column + i);
    match at.get(start..end) {
        Some(w) if !w.is_empty() => Ok(w.to_string()),
        _ => Err(Error::invalid(format!(
            "no identifier at {file}:{line}:{column}"
        ))),
    }
}

/// The byte column of the first whole-word `name` on `line`.
fn word_column(line: &str, name: &str) -> Option<usize> {
    let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    line.match_indices(name).map(|(i, _)| i).find(|&i| {
        !word(line[..i].chars().next_back()) && !word(line[i + name.len()..].chars().next())
    })
}

#[cfg(test)]
mod tests {
    use super::word_column;

    #[test]
    fn word_columns_skip_partial_matches() {
        assert_eq!(word_column("helpers(); helper();", "helper"), Some(11));
        assert_eq!(word_column("nothing", "helper"), None);
    }
}
