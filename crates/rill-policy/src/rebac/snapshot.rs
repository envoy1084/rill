//! A point-in-time copy of a repository's relationship tuples.

use std::sync::Arc;
use std::time::{Duration, Instant};

use rill_store::RepoStore;
use rill_types::{Principal, RepoPathBuf};

use super::model::{dir_of_object, inheritance_chain, Relation, Subject};
use crate::error::Result;

/// One relationship tuple.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tuple {
    /// Object id, e.g. `repo` or `dir:/src/billing`.
    pub object: String,
    /// The relation granted.
    pub relation: Relation,
    /// Who it is granted to.
    pub subject: Subject,
}

/// All tuples of a repository as of one scan.
#[derive(Debug)]
pub struct Snapshot {
    tuples: Vec<Tuple>,
    taken_at: Instant,
}

impl Snapshot {
    /// Build a snapshot from tuples (for tests and offline evaluation).
    pub fn from_tuples(tuples: Vec<Tuple>) -> Self {
        Snapshot {
            tuples,
            taken_at: Instant::now(),
        }
    }

    /// Read every relationship tuple of the repository. Tuples that do
    /// not parse are logged and skipped rather than failing every check in
    /// the repository.
    pub async fn load(repo: &RepoStore) -> Result<Self> {
        let rows = repo.list_relations().await?;
        let mut tuples = Vec::with_capacity(rows.len());
        for (object, relation, subject) in rows {
            match parse_tuple(&object, &relation, &subject) {
                Ok(t) => tuples.push(t),
                Err(e) => {
                    tracing::warn!(error = %e, %object, %relation, %subject, "skipping malformed relation tuple")
                }
            }
        }
        Ok(Snapshot::from_tuples(tuples))
    }

    /// Whether the snapshot is older than `ttl`.
    pub fn is_stale(&self, ttl: Duration) -> bool {
        self.taken_at.elapsed() > ttl
    }

    /// All tuples.
    pub fn tuples(&self) -> &[Tuple] {
        &self.tuples
    }

    /// Whether `principal` holds `relation` on `object`, directly or through
    /// a rewrite or an ancestor directory.
    pub fn check(
        &self,
        principal: &Principal,
        teams: &[String],
        relation: Relation,
        object: &str,
    ) -> bool {
        let accepted = relation.satisfied_by();
        let chain = inheritance_chain(object);
        self.tuples.iter().any(|t| {
            accepted.contains(&t.relation)
                && chain.contains(&t.object)
                && t.subject.matches(principal, teams)
        })
    }

    /// Directories (as paths; the root for `repo`) on which `principal`
    /// directly holds a tuple satisfying `relation`. Each returned path
    /// implies its whole subtree.
    pub fn granted_dirs(
        &self,
        principal: &Principal,
        teams: &[String],
        relation: Relation,
    ) -> Vec<RepoPathBuf> {
        let accepted = relation.satisfied_by();
        let mut dirs: Vec<RepoPathBuf> = self
            .tuples
            .iter()
            .filter(|t| accepted.contains(&t.relation) && t.subject.matches(principal, teams))
            .filter_map(|t| dir_of_object(&t.object))
            .collect();
        dirs.sort();
        dirs.dedup();
        dirs
    }
}

fn parse_tuple(object: &str, relation: &str, subject: &str) -> Result<Tuple> {
    Ok(Tuple {
        object: object.to_string(),
        relation: relation.parse()?,
        subject: subject.parse()?,
    })
}

/// A cache holding one snapshot per repository for a short time, so a burst
/// of checks during one request shares a single scan.
#[derive(Debug)]
pub struct SnapshotCache {
    ttl: Duration,
    current: std::sync::Mutex<Option<Arc<Snapshot>>>,
}

impl SnapshotCache {
    /// Cache with the given time-to-live.
    pub fn new(ttl: Duration) -> Self {
        SnapshotCache {
            ttl,
            current: std::sync::Mutex::new(None),
        }
    }

    /// The current snapshot, reloading it when stale or absent.
    pub async fn get(&self, repo: &RepoStore) -> Result<Arc<Snapshot>> {
        if let Some(snap) = self.peek() {
            return Ok(snap);
        }
        let fresh = Arc::new(Snapshot::load(repo).await?);
        *self.lock() = Some(fresh.clone());
        Ok(fresh)
    }

    /// Drop the cached snapshot so the next check sees new tuples.
    pub fn invalidate(&self) {
        *self.lock() = None;
    }

    fn peek(&self) -> Option<Arc<Snapshot>> {
        let guard = self.lock();
        guard.as_ref().filter(|s| !s.is_stale(self.ttl)).cloned()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Arc<Snapshot>>> {
        // The guard only ever wraps an Arc swap; a poisoned lock holds
        // nothing worse than a stale snapshot.
        self.current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn objects_with_slashes_load_exactly() {
        let stores = rill_store::Stores::memory();
        let repo = stores.repo(stores.create_repo("r", "").await.unwrap().id);
        repo.add_relation(
            "dir:/src/billing",
            "owner",
            "team:core",
            &Principal::human("admin"),
        )
        .await
        .unwrap();
        repo.add_relation("dir:/src", "reader", "*", &Principal::human("admin"))
            .await
            .unwrap();
        repo.add_relation("repo", "boss", "*", &Principal::human("admin"))
            .await
            .unwrap();
        let snapshot = Snapshot::load(&repo).await.unwrap();
        let mut tuples: Vec<_> = snapshot
            .tuples()
            .iter()
            .map(|t| (t.object.as_str(), t.relation, t.subject.clone()))
            .collect();
        tuples.sort_by(|a, b| a.0.cmp(b.0));
        assert_eq!(
            tuples,
            vec![
                ("dir:/src", Relation::Reader, Subject::Any),
                (
                    "dir:/src/billing",
                    Relation::Owner,
                    Subject::Team("core".into())
                ),
            ]
        );
    }
}
