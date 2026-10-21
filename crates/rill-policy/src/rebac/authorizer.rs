//! The ReBAC authorizer: relationship checks over a repository's tuples.

use std::sync::Arc;
use std::time::Duration;

use rill_store::RepoStore;
use rill_types::{Principal, RepoPath, RepoPathBuf};

use super::model::{dir_object, Relation, Subject};
use super::snapshot::{Snapshot, SnapshotCache};
use crate::error::Result;

/// How long a snapshot of the tuple set is reused before it is re-read.
pub const DEFAULT_SNAPSHOT_TTL: Duration = Duration::from_millis(500);

/// A predicate over repository paths: which ones a principal may see.
///
/// Built once per request from the tuple snapshot and then applied to every
/// candidate path (sparse profile entries, search hits, tree listings)
/// without touching the store again.
#[derive(Clone)]
pub struct PathFilter {
    inner: Arc<dyn Fn(&RepoPath) -> bool + Send + Sync>,
}

impl PathFilter {
    /// A filter from a closure.
    pub fn new(f: impl Fn(&RepoPath) -> bool + Send + Sync + 'static) -> Self {
        PathFilter { inner: Arc::new(f) }
    }

    /// Allow everything.
    pub fn allow_all() -> Self {
        PathFilter::new(|_| true)
    }

    /// Deny everything.
    pub fn deny_all() -> Self {
        PathFilter::new(|_| false)
    }

    /// Allow paths at or below any of `roots`. An empty list denies all; a
    /// list containing the root allows all.
    pub fn under(roots: Vec<RepoPathBuf>) -> Self {
        if roots.iter().any(|r| r.is_root()) {
            return Self::allow_all();
        }
        PathFilter::new(move |p| roots.iter().any(|r| p.starts_with(r)))
    }

    /// Whether `path` passes the filter.
    pub fn allows(&self, path: &RepoPath) -> bool {
        (self.inner)(path)
    }

    /// Keep only the paths that pass.
    pub fn retain<P: AsRef<RepoPath>>(&self, paths: &mut Vec<P>) {
        paths.retain(|p| self.allows(p.as_ref()));
    }
}

impl std::fmt::Debug for PathFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PathFilter")
    }
}

/// Relationship-based authorizer for one repository.
///
/// Reads tuples through a short-lived [`SnapshotCache`] so a request that
/// performs many checks scans the store once. Writes go through the
/// [`RepoStore`] directly (`add_relation` / `remove_relation`) and then
/// [`Authorizer::invalidate`] drops the cached snapshot.
pub struct Authorizer {
    repo: RepoStore,
    cache: SnapshotCache,
}

impl std::fmt::Debug for Authorizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Authorizer")
            .field("repo", &self.repo.id())
            .finish_non_exhaustive()
    }
}

impl Authorizer {
    /// Authorizer with the default snapshot TTL.
    pub fn new(repo: RepoStore) -> Self {
        Self::with_ttl(repo, DEFAULT_SNAPSHOT_TTL)
    }

    /// Authorizer with an explicit snapshot TTL.
    pub fn with_ttl(repo: RepoStore, ttl: Duration) -> Self {
        Authorizer {
            repo,
            cache: SnapshotCache::new(ttl),
        }
    }

    /// The repository this authorizer guards.
    pub fn repo(&self) -> &RepoStore {
        &self.repo
    }

    /// Grant `relation` on `object` to `subject`, on `by`'s authority.
    pub async fn grant(
        &self,
        object: &str,
        relation: Relation,
        subject: &Subject,
        by: &Principal,
    ) -> Result<()> {
        self.repo
            .add_relation(object, relation.as_str(), &subject.to_string(), by)
            .await?;
        self.invalidate();
        Ok(())
    }

    /// Revoke `relation` on `object` from `subject`, on `by`'s authority.
    pub async fn revoke(
        &self,
        object: &str,
        relation: Relation,
        subject: &Subject,
        by: &Principal,
    ) -> Result<()> {
        self.repo
            .remove_relation(object, relation.as_str(), &subject.to_string(), by)
            .await?;
        self.invalidate();
        Ok(())
    }

    /// Drop the cached snapshot so the next check re-reads the store.
    pub fn invalidate(&self) {
        self.cache.invalidate();
    }

    /// The current snapshot, for callers that need to run many checks.
    pub async fn snapshot(&self) -> Result<Arc<Snapshot>> {
        self.cache.get(&self.repo).await
    }

    /// Whether `principal` (a member of `teams`) holds `relation` on
    /// `object`. Errors reading the store are logged and treated as denial.
    pub async fn check(
        &self,
        principal: &Principal,
        teams: &[String],
        relation: Relation,
        object: &str,
    ) -> bool {
        match self.snapshot().await {
            Ok(snap) => snap.check(principal, teams, relation, object),
            Err(e) => {
                tracing::error!(error = %e, repo = %self.repo.id(), "relation check failed; denying");
                false
            }
        }
    }

    /// Whether `principal` holds `relation` on the directory containing
    /// `path` (or on `path` itself if it is a directory).
    pub async fn check_path(
        &self,
        principal: &Principal,
        teams: &[String],
        relation: Relation,
        path: &RepoPath,
    ) -> bool {
        self.check(principal, teams, relation, &dir_object(path))
            .await
    }

    /// Paths `principal` may read, for filtering sparse profiles and search
    /// results. A store error yields a filter that denies everything.
    pub async fn allowed_paths(&self, principal: &Principal, teams: &[String]) -> PathFilter {
        self.allowed_paths_for(principal, teams, Relation::Reader)
            .await
    }

    /// Paths on which `principal` holds `relation`.
    pub async fn allowed_paths_for(
        &self,
        principal: &Principal,
        teams: &[String],
        relation: Relation,
    ) -> PathFilter {
        match self.snapshot().await {
            Ok(snap) => PathFilter::under(snap.granted_dirs(principal, teams, relation)),
            Err(e) => {
                tracing::error!(error = %e, repo = %self.repo.id(), "loading relations failed; denying all paths");
                PathFilter::deny_all()
            }
        }
    }
}
