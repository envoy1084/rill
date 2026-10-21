//! The ReBAC authorizer: relationship checks over a repository's tuples.

use std::sync::Arc;
use std::time::Duration;

use rill_store::RepoStore;
use rill_types::{Principal, RepoPath};

use super::cache::{SnapshotCache, DEFAULT_SNAPSHOT_TTL};
use super::model::{dir_object, Relation, Subject};
use super::path_filter::PathFilter;
use super::snapshot::Snapshot;
use crate::error::Result;

/// Relationship-based authorizer for one repository.
///
/// Checks run against a cached [`Snapshot`]. Grants and revokes through
/// the authorizer apply at once on this server; servers drop their
/// snapshot when the event log reports a relation change elsewhere, and a
/// caller that knows a newer relation version asks for it explicitly.
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

    /// Authorizer trusting a snapshot for at most `ttl` between reloads.
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

    /// Grant `relation` on `object` to `subject`, on `by`'s authority;
    /// returns the relation version that includes it.
    pub async fn grant(
        &self,
        object: &str,
        relation: Relation,
        subject: &Subject,
        by: &Principal,
    ) -> Result<i64> {
        let version = self
            .repo
            .add_relation(object, relation.as_str(), &subject.to_string(), by)
            .await?;
        self.invalidate();
        Ok(version)
    }

    /// Revoke `relation` on `object` from `subject`, on `by`'s authority;
    /// returns the relation version that includes it.
    pub async fn revoke(
        &self,
        object: &str,
        relation: Relation,
        subject: &Subject,
        by: &Principal,
    ) -> Result<i64> {
        let version = self
            .repo
            .remove_relation(object, relation.as_str(), &subject.to_string(), by)
            .await?;
        self.invalidate();
        Ok(version)
    }

    /// Drop the cached snapshot so the next check re-reads the store.
    pub fn invalidate(&self) {
        self.cache.invalidate();
    }

    /// The current snapshot.
    pub async fn snapshot(&self) -> Result<Arc<Snapshot>> {
        self.cache.get(&self.repo, 0).await
    }

    /// A snapshot reflecting at least relation version `at_least`.
    pub async fn snapshot_at_least(&self, at_least: i64) -> Result<Arc<Snapshot>> {
        self.cache.get(&self.repo, at_least).await
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

    /// Whether `principal` holds `relation` on the directory `path` (the
    /// repository for the root).
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

    /// Paths `principal` may read.
    pub async fn allowed_paths(&self, principal: &Principal, teams: &[String]) -> PathFilter {
        self.allowed_paths_for(principal, teams, Relation::Reader)
            .await
    }

    /// Paths on which `principal` holds `relation`. A store error yields a
    /// filter that denies everything.
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
