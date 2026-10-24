//! The server side of code search: which files a revision's search covers
//! (its indexed root, or the nearest indexed ancestor plus the files that
//! changed since, read live), how fresh the index is, and running it.

use std::sync::Arc;
use std::time::Duration;

use rill_core::{diff_trees, RootTree};
use rill_search::{LiveFile, Scope, SearchQuery, ShardResults, ShardSelect};
use rill_types::{ChangeVersionHash, TreeHash, TrunkSeq};

use super::RepoContext;
use crate::error::{Error, Result};

/// How long a search waits for a root it asked to have indexed.
const INDEX_WAIT: Duration = Duration::from_secs(30);
/// How often a waiting search looks again.
const INDEX_POLL: Duration = Duration::from_millis(50);
/// Ancestors searched for an indexed root before asking for a full index.
const MAX_ANCESTORS: usize = 64;
/// Files that may be read live instead of from the index.
const MAX_LIVE_FILES: usize = 2_000;

impl RepoContext {
    /// What a search of `root` (of `version`, when it has one) covers.
    /// With `min_seq`, first wait until trunk is indexed through that
    /// position.
    pub async fn search_scope(
        &self,
        version: Option<ChangeVersionHash>,
        root: TreeHash,
        min_seq: Option<TrunkSeq>,
    ) -> Result<Scope> {
        if let Some(seq) = min_seq {
            self.wait_for_seq(seq).await?;
        }
        self.scope(version, root).await
    }

    /// The matched files of the shards `select` names, at paths `allowed`
    /// admits.
    pub async fn grep_shards(
        &self,
        scope: Scope,
        query: SearchQuery,
        allowed: rill_policy::PathFilter,
        select: ShardSelect,
    ) -> Result<ShardResults> {
        let search = self.search.clone();
        // Boxed so the borrow-heavy search future has a concrete `Send`
        // bound for the handler that awaits it.
        let fut: std::pin::Pin<
            Box<dyn std::future::Future<Output = rill_search::Result<ShardResults>> + Send>,
        > = Box::pin(async move {
            rill_search::search_shards(&search, &scope, &query, move |p| allowed.allows(p), &select)
                .await
        });
        Ok(fut.await?)
    }

    /// What a search of `root` covers.
    async fn scope(&self, version: Option<ChangeVersionHash>, root: TreeHash) -> Result<Scope> {
        self.search.refresh().await?;
        if let Some(view) = self.search.view(root).await? {
            return Ok(Scope::indexed(view));
        }
        if let Some(version) = version {
            if let Some(scope) = self.overlay(version, root).await? {
                return Ok(scope);
            }
        }
        self.wait_for_index(root).await?;
        let view = self
            .search
            .view(root)
            .await?
            .ok_or(rill_search::Error::RootNotIndexed(root))?;
        Ok(Scope::indexed(view))
    }

    /// The nearest indexed ancestor of `version` (first parents), with the
    /// files changed since read live; `None` when none is near or too much
    /// changed.
    async fn overlay(&self, version: ChangeVersionHash, root: TreeHash) -> Result<Option<Scope>> {
        let mut at = version;
        for _ in 0..MAX_ANCESTORS {
            let Some(parent) = self.derive.change(&at).await?.first_parent() else {
                return Ok(None);
            };
            let base = self.derive.get::<RootTree>(&parent).await?;
            if let Some(view) = self.search.view(base).await? {
                let entries = diff_trees(&self.trees, Some(base), Some(root)).await?;
                if entries.len() > MAX_LIVE_FILES {
                    return Ok(None);
                }
                let changes: Vec<_> = entries
                    .iter()
                    .map(|e| {
                        (
                            e.path.clone(),
                            e.after.as_ref().map(|(c, k)| (c.content_hash(), *k)),
                        )
                    })
                    .collect();
                let live = entries
                    .into_iter()
                    .filter_map(|e| {
                        e.after.map(|(content, _)| LiveFile {
                            path: e.path,
                            content,
                        })
                    })
                    .collect();
                return Ok(Some(Scope {
                    view: Arc::new(view.apply(&changes)),
                    live,
                }));
            }
            at = parent;
        }
        Ok(None)
    }

    /// Wait until trunk is indexed through `seq`.
    async fn wait_for_seq(&self, seq: TrunkSeq) -> Result<()> {
        let deadline = tokio::time::Instant::now() + INDEX_WAIT;
        loop {
            self.search.refresh().await?;
            let through = self.search.through();
            if through.is_some_and(|t| t.seq >= seq) {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                let at = through.map_or(0, |t| t.seq.0);
                return Err(Error::Unavailable(format!(
                    "the search index is at trunk@{at}, not yet trunk@{}; retry",
                    seq.0
                )));
            }
            tokio::time::sleep(INDEX_POLL).await;
        }
    }

    /// Ask the search writer to index `root` and wait for it: only the
    /// server holding the search lease writes the index.
    async fn wait_for_index(&self, root: TreeHash) -> Result<()> {
        let _request = match self.index_requests.claim(root) {
            Some(request) => {
                rill_jobs::Queue::new(self.repo.clone(), rill_lander::land::followups::INDEX_QUEUE)
                    .push(rill_lander::land::followups::index_root_job(&root))
                    .await?;
                Some(request)
            }
            None => None,
        };
        let deadline = tokio::time::Instant::now() + INDEX_WAIT;
        while tokio::time::Instant::now() < deadline {
            self.search.refresh().await?;
            if self.search.has_root(root).await? {
                return Ok(());
            }
            tokio::time::sleep(INDEX_POLL).await;
        }
        Err(rill_search::Error::RootNotIndexed(root).into())
    }
}
