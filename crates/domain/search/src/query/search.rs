//! The search pipeline: in every shard at once, each segment's trigram
//! candidates are kept when the scope holds them at a permitted path, then
//! verified against the stored content; live files are read and scanned;
//! matched files are ranked and their hits returned best first.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use futures::stream::{self, StreamExt};
use rill_core::read_content;
use rill_store::ObjectStore;
use rill_types::{ObjectHash, ObjectKind, RepoPath, RepoPathBuf};

use super::hit::SearchHit;
use super::lang::language_of;
use super::lines::LineIndex;
use super::matcher::Matcher;
use super::rank::{score, Signals};
use super::request::SearchQuery;
use super::result::{Facets, SearchResult};
use super::scope::Scope;
use crate::error::Result;
use crate::index::SearchIndex;
use crate::paths::PathView;
use crate::segment::Segment;
use crate::trigram::{Candidates, TrigramQuery};

/// Permission hook: decides whether a path may appear in results. The
/// caller (the server's ReBAC layer) supplies it; the search never reads
/// a blob whose every path is filtered out.
pub trait PathFilter: Fn(&RepoPath) -> bool + Send + Sync {}

impl<F: Fn(&RepoPath) -> bool + Send + Sync> PathFilter for F {}

/// Matched files collected per shard before ranking, as a multiple of the
/// hits asked for (at least 50).
const RANK_POOL: usize = 2;

/// One matched file with its score, before the files are merged.
struct FileMatch {
    path: RepoPathBuf,
    lines: Vec<RawHit>,
    score: i32,
}

/// A matched file from one set of shards, ready to merge with others.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankedFile {
    pub path: RepoPathBuf,
    pub score: i32,
    pub hits: Vec<SearchHit>,
}

/// Which shards a search covers, and whether it reads the live files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardSelect {
    /// `None` for every shard.
    pub shards: Option<Vec<usize>>,
    pub live: bool,
}

impl ShardSelect {
    pub fn all() -> ShardSelect {
        ShardSelect {
            shards: None,
            live: true,
        }
    }
}

/// The matched files of some shards.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShardResults {
    pub files: Vec<RankedFile>,
    /// A shard stopped at its file limit.
    pub capped: bool,
    /// The time budget ran out.
    pub partial: bool,
}

fn ranked(f: FileMatch) -> RankedFile {
    RankedFile {
        hits: f
            .lines
            .into_iter()
            .map(|(line_no, line, before, after, byte_range)| SearchHit {
                path: f.path.clone(),
                line_no,
                line,
                before,
                after,
                byte_range,
            })
            .collect(),
        path: f.path,
        score: f.score,
    }
}

type RawHit = (
    u32,
    String,
    Vec<String>,
    Vec<String>,
    std::ops::Range<usize>,
);

/// Everything a shard needs, shared across the blocking tasks.
struct Job<F> {
    view: Arc<PathView>,
    query: SearchQuery,
    matcher: Matcher,
    plan: TrigramQuery,
    allow: F,
    objects: Arc<dyn ObjectStore>,
    deadline: Instant,
    pool: usize,
    /// Paths the live files take over: never answered from the index.
    shadowed: HashSet<RepoPathBuf>,
}

impl<F: PathFilter> Job<F> {
    fn wanted(&self, path: &RepoPath) -> bool {
        !self.shadowed.contains(path)
            && self.query.path_allowed(path.as_str())
            && self.query.language_allowed(path.as_str())
            && (self.allow)(path)
    }

    fn redacted(&self, hash: &ObjectHash) -> bool {
        self.objects.refuses(ObjectKind::Blob, hash).is_err()
    }

    fn rank(&self, path: &RepoPathBuf, lines: Vec<RawHit>, defines: bool) -> FileMatch {
        let name = path.as_str().rsplit('/').next().unwrap_or_default();
        let s = score(&Signals {
            path,
            name_matches: self.matcher.matches_name(name),
            defines,
            recent: self.view.recently_changed(path),
        });
        FileMatch {
            path: path.clone(),
            lines,
            score: s,
        }
    }

    /// The matched files of one shard's segments.
    fn shard(&self, segments: &[Arc<Segment>]) -> Result<(Vec<FileMatch>, bool)> {
        let mut out = Vec::new();
        for seg in segments {
            let candidates = seg.candidates(&self.plan)?;
            let docs: Box<dyn Iterator<Item = u32>> = match &candidates {
                Candidates::All => Box::new(0..seg.docs().len() as u32),
                Candidates::Docs(d) => Box::new(d.iter()),
            };
            for doc in docs {
                if Instant::now() > self.deadline {
                    return Ok((out, true));
                }
                let hash = seg.docs()[doc as usize].hash;
                if !self.view.holds(&hash) {
                    continue;
                }
                let paths: Vec<RepoPathBuf> = self
                    .view
                    .paths_of(&hash)
                    .into_iter()
                    .filter(|p| self.wanted(p))
                    .collect();
                if paths.is_empty() || self.redacted(&hash) {
                    continue;
                }
                let content = seg.content(doc)?;
                let lines = scan_blob(
                    &self.matcher,
                    paths[0].as_str(),
                    &content,
                    self.query.context_lines,
                    self.query.max_results,
                );
                if lines.is_empty() {
                    continue;
                }
                let defs: HashSet<u32> = seg.symbols(doc)?.into_iter().map(|s| s.line).collect();
                let defines = lines.iter().any(|l| defs.contains(&l.0));
                for path in &paths {
                    out.push(self.rank(path, lines.clone(), defines));
                }
                if out.len() >= self.pool {
                    return Ok((out, true));
                }
            }
        }
        Ok((out, false))
    }
}

/// Search `scope` for `query`, best files first.
pub async fn search<F: PathFilter + Clone + 'static>(
    index: &SearchIndex,
    scope: &Scope,
    query: &SearchQuery,
    allow: F,
) -> Result<SearchResult> {
    let found = search_shards(index, scope, query, allow, &ShardSelect::all()).await?;
    Ok(assemble(
        found.files,
        query.max_results,
        found.capped,
        found.partial,
    ))
}

/// The matched files of the shards `select` names (and the live files,
/// when it says so), for merging with other shards' with [`assemble`].
pub async fn search_shards<F: PathFilter + Clone + 'static>(
    index: &SearchIndex,
    scope: &Scope,
    query: &SearchQuery,
    allow: F,
    select: &ShardSelect,
) -> Result<ShardResults> {
    index.refresh().await?;
    let matcher = query.matcher()?;
    let job = Arc::new(Job {
        view: scope.view.clone(),
        matcher: matcher.clone(),
        plan: matcher.trigram_query()?,
        query: query.clone(),
        allow,
        objects: index.repo().objects().clone(),
        deadline: Instant::now() + query.time_budget,
        pool: query.max_results.max(50) * RANK_POOL,
        shadowed: scope.live.iter().map(|f| f.path.clone()).collect(),
    });
    let shards: Vec<usize> = match &select.shards {
        Some(s) => s.clone(),
        None => (0..index.config().shards.max(1) as usize).collect(),
    };
    let mut tasks = Vec::with_capacity(shards.len());
    for shard in shards {
        let segments = index.segments_of(shard).await?;
        let job = job.clone();
        tasks.push(tokio::task::spawn_blocking(move || job.shard(&segments)));
    }
    let mut files = Vec::new();
    let mut capped = false;
    for t in tasks {
        let (found, cut) = t.await??;
        capped |= cut;
        files.extend(found);
    }
    let partial = Instant::now() > job.deadline;
    if select.live {
        files.extend(live(&job, scope).await?);
    }
    Ok(ShardResults {
        files: files.into_iter().map(ranked).collect(),
        capped,
        partial,
    })
}

/// Matches in the live files.
async fn live<F: PathFilter + 'static>(job: &Arc<Job<F>>, scope: &Scope) -> Result<Vec<FileMatch>> {
    let wanted: Vec<_> = scope
        .live
        .iter()
        .filter(|f| {
            job.query.path_allowed(f.path.as_str())
                && job.query.language_allowed(f.path.as_str())
                && (job.allow)(f.path.as_path())
        })
        .cloned()
        .collect();
    let mut out = Vec::new();
    let mut scans = stream::iter(wanted)
        .map(|f| {
            let job = job.clone();
            async move {
                let bytes = match read_content(job.objects.as_ref(), &f.content).await {
                    Err(e) if e.is_redacted() => bytes::Bytes::new(),
                    other => other?,
                };
                if rill_core::is_binary(&bytes) {
                    return Ok::<_, crate::Error>(None);
                }
                let (matcher, context, max) = (
                    job.matcher.clone(),
                    job.query.context_lines,
                    job.query.max_results,
                );
                let path = f.path.to_string();
                let lines = tokio::task::spawn_blocking(move || {
                    scan_blob(&matcher, &path, &bytes, context, max)
                })
                .await?;
                Ok((!lines.is_empty()).then(|| job.rank(&f.path, lines, false)))
            }
        })
        .buffered(16);
    while let Some(m) = scans.next().await {
        out.extend(m?);
    }
    Ok(out)
}

/// Merge matched files (from any number of shard searches): rank, count
/// facets and flatten into at most `max` hits.
pub fn assemble(
    mut files: Vec<RankedFile>,
    max: usize,
    capped: bool,
    partial: bool,
) -> SearchResult {
    files.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
    files.dedup_by(|a, b| a.path == b.path);
    let mut facets = Facets::default();
    for f in &files {
        let top = match f.path.as_str().split_once('/') {
            Some((dir, _)) => dir.to_string(),
            None => ".".to_string(),
        };
        *facets.directories.entry(top).or_default() += 1;
        *facets
            .languages
            .entry(language_of(f.path.as_str()).to_string())
            .or_default() += 1;
    }
    let total: usize = files.iter().map(|f| f.hits.len()).sum();
    let hits: Vec<SearchHit> = files
        .iter()
        .flat_map(|f| f.hits.iter().cloned())
        .take(max)
        .collect();
    SearchResult {
        truncated: capped || total > hits.len(),
        files: files.len() as u32,
        hits,
        partial,
        facets,
    }
}

/// Matching lines of one blob, at most `limit` of them.
fn scan_blob(
    matcher: &Matcher,
    path: &str,
    bytes: &[u8],
    context: usize,
    limit: usize,
) -> Vec<RawHit> {
    let mut out = Vec::new();
    let mut lines: Option<LineIndex> = None;
    let mut last_line: Option<usize> = None;
    for m in matcher.find(path, bytes) {
        if out.len() >= limit {
            break;
        }
        let idx = lines.get_or_insert_with(|| LineIndex::new(bytes));
        let line = idx.line_of(m.start);
        if last_line == Some(line) {
            continue;
        }
        last_line = Some(line);
        out.push((
            (line + 1) as u32,
            idx.line_string(line),
            idx.before(line, context),
            idx.after(line, context),
            m,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    use regex::bytes::Regex;

    #[test]
    fn one_hit_per_line_with_context() {
        let re = Matcher::Regex {
            regex: Regex::new("a").unwrap(),
            source: "a".into(),
        };
        let hits = scan_blob(&re, "f", b"x\naa\ny\na\n", 1, 10);
        assert_eq!(hits.len(), 2);
        assert_eq!((hits[0].0, hits[0].1.as_str()), (2, "aa"));
        assert_eq!(hits[0].2, vec!["x"]);
        assert_eq!(hits[0].3, vec!["y"]);
        assert_eq!(hits[0].4, 2..3);
        assert_eq!(scan_blob(&re, "f", b"a\na\na\n", 0, 2).len(), 2);
    }
}
