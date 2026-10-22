//! A versioned, indexed copy of a repository's relationship tuples.

use std::collections::HashMap;

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

/// All tuples of a repository as of relation version `version`.
#[derive(Debug, Default)]
pub struct Snapshot {
    version: i64,
    tuples: Vec<Tuple>,
    by_object: HashMap<String, Vec<usize>>,
}

impl Snapshot {
    /// A snapshot of `tuples` at `version` (for tests and offline
    /// evaluation).
    pub fn from_tuples(version: i64, tuples: Vec<Tuple>) -> Self {
        let mut by_object: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, t) in tuples.iter().enumerate() {
            by_object.entry(t.object.clone()).or_default().push(i);
        }
        Snapshot {
            version,
            tuples,
            by_object,
        }
    }

    /// Read the repository's tuples. The version is read first, so the
    /// tuples reflect at least that many changes. Tuples that do not parse
    /// are logged and skipped rather than failing every check.
    pub async fn load(repo: &RepoStore) -> Result<Self> {
        let version = repo.relations_version().await?;
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
        Ok(Snapshot::from_tuples(version, tuples))
    }

    /// The relation version the snapshot reflects.
    pub fn version(&self) -> i64 {
        self.version
    }

    /// All tuples.
    pub fn tuples(&self) -> &[Tuple] {
        &self.tuples
    }

    /// The tuples on `object` and its ancestors that satisfy `relation`,
    /// nearest object first: why someone holds it.
    pub fn grants_for(&self, relation: Relation, object: &str) -> Vec<&Tuple> {
        let accepted = relation.satisfied_by();
        inheritance_chain(object)
            .iter()
            .filter_map(|o| self.by_object.get(o))
            .flatten()
            .map(|&i| &self.tuples[i])
            .filter(|t| accepted.contains(&t.relation))
            .collect()
    }

    /// Whether `principal` holds `relation` on `object`, directly or through
    /// a stronger relation or an ancestor directory.
    pub fn check(
        &self,
        principal: &Principal,
        teams: &[String],
        relation: Relation,
        object: &str,
    ) -> bool {
        self.grants_for(relation, object)
            .iter()
            .any(|t| t.subject.matches(principal, teams))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn objects_with_slashes_load_exactly() {
        let stores = rill_store::Stores::memory();
        let repo = stores.repo(stores.create_repo("r", "").await.unwrap().id);
        let admin = Principal::human("admin");
        repo.add_relation("dir:/src/billing", "owner", "team:core", &admin)
            .await
            .unwrap();
        repo.add_relation("dir:/src", "reader", "*", &admin)
            .await
            .unwrap();
        repo.add_relation("repo", "boss", "*", &admin)
            .await
            .unwrap();
        let snapshot = Snapshot::load(&repo).await.unwrap();
        assert_eq!(snapshot.version(), 3);
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
        let why: Vec<_> = snapshot
            .grants_for(Relation::Reader, "dir:/src/billing/x")
            .into_iter()
            .map(|t| t.object.as_str())
            .collect();
        assert_eq!(why, ["dir:/src/billing", "dir:/src"], "nearest first");
    }
}
