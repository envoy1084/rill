//! The search pipeline: trigram candidates, regex over candidate blobs,
//! hits mapped back to paths.

use std::sync::Arc;

use futures::stream::{self, StreamExt};
use regex::bytes::Regex;
use rill_core::read_content;
use rill_types::{RepoPath, TreeHash};

use super::hit::SearchHit;
use super::lines::LineIndex;
use super::request::SearchQuery;
use crate::error::Result;
use crate::index::{ManifestEntry, SearchIndex};
use crate::trigram::Candidates;

/// Permission hook: decides whether a path may appear in results. The
/// caller (typically the server's ReBAC layer) supplies it; the search
/// never reads a blob whose path is filtered out.
pub trait PathFilter: Fn(&RepoPath) -> bool + Send + Sync {}

impl<F: Fn(&RepoPath) -> bool + Send + Sync> PathFilter for F {}

/// Search `root` for `query`, returning hits in path then line order.
///
/// Files are visited in manifest (path) order; the regex runs only over
/// blobs that the trigram query admits and that `allow` and the path glob
/// accept. The result is cut at `query.max_results`.
pub async fn search<F: PathFilter>(
    index: &SearchIndex,
    root: TreeHash,
    query: &SearchQuery,
    allow: F,
) -> Result<Vec<SearchHit>> {
    let regex = Arc::new(query.compile()?);
    let manifest = index.manifest(root).await?;
    // The manifest is written after the postings behind it, so catching
    // up now covers every file it lists.
    index.refresh().await?;
    let candidates = query.trigram_query()?.evaluate(&|t| index.postings_of(t));

    // Owned entries: a stream over borrowed entries makes the future's
    // `Send` bound depend on a higher-ranked lifetime, which callers that
    // box or spawn this future cannot satisfy.
    let files: Vec<ManifestEntry> = manifest
        .entries
        .iter()
        .filter(|e| query.path_allowed(e.path.as_str()) && allow(e.path.as_path()))
        .filter(|e| match &candidates {
            // Blobs without a document id were skipped at index time
            // (binary or too large) and are never searched.
            Candidates::All => index.doc_of(&e.content_hash()).is_some(),
            Candidates::Docs(_) => index
                .doc_of(&e.content_hash())
                .is_some_and(|d| candidates.contains(d)),
        })
        .cloned()
        .collect();
    tracing::debug!(
        root = %root,
        files = manifest.len(),
        candidates = files.len(),
        pruned = candidates != Candidates::All,
        "search"
    );

    let objects = index.repo().objects().clone();
    let max = query.max_results;
    let context = query.context_lines;
    let mut results = stream::iter(files)
        .map(|entry| {
            let objects = objects.clone();
            let regex = regex.clone();
            async move {
                let bytes = match read_content(objects.as_ref(), &entry.content).await {
                    Err(e) if e.is_redacted() => bytes::Bytes::new(),
                    other => other?,
                };
                let hits =
                    tokio::task::spawn_blocking(move || scan_blob(&regex, &bytes, context, max))
                        .await?;
                Ok::<_, crate::error::Error>((entry.path, hits))
            }
        })
        .buffered(16);

    let mut out: Vec<SearchHit> = Vec::new();
    while let Some(item) = results.next().await {
        let (path, hits) = item?;
        for (line_no, line, before, after, byte_range) in hits {
            if out.len() >= max {
                return Ok(out);
            }
            out.push(SearchHit {
                path: path.clone(),
                line_no,
                line,
                before,
                after,
                byte_range,
            });
        }
    }
    Ok(out)
}

type RawHit = (
    u32,
    String,
    Vec<String>,
    Vec<String>,
    std::ops::Range<usize>,
);

/// Matching lines of one blob, at most `limit` of them.
fn scan_blob(regex: &Regex, bytes: &[u8], context: usize, limit: usize) -> Vec<RawHit> {
    let mut out = Vec::new();
    let mut lines: Option<LineIndex> = None;
    let mut last_line: Option<usize> = None;
    for m in regex.find_iter(bytes) {
        if out.len() >= limit {
            break;
        }
        let idx = lines.get_or_insert_with(|| LineIndex::new(bytes));
        let line = idx.line_of(m.start());
        if last_line == Some(line) {
            continue;
        }
        last_line = Some(line);
        out.push((
            (line + 1) as u32,
            idx.line_string(line),
            idx.before(line, context),
            idx.after(line, context),
            m.range(),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_hit_per_line_with_context() {
        let re = Regex::new("a").unwrap();
        let hits = scan_blob(&re, b"x\naa\ny\na\n", 1, 10);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].0, 2);
        assert_eq!(hits[0].1, "aa");
        assert_eq!(hits[0].2, vec!["x"]);
        assert_eq!(hits[0].3, vec!["y"]);
        assert_eq!(hits[0].4, 2..3);
        assert_eq!(hits[1].0, 4);
        assert_eq!(scan_blob(&re, b"a\na\na\n", 0, 2).len(), 2);
    }
}
