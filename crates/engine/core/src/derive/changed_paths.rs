//! The paths a version changes relative to its first parent.
//!
//! Read straight from the change: every file change and subtree change
//! path, so computing it never diffs trees. It is a superset of the paths
//! whose state differs (a rewrite with the same content counts) and names
//! a copied or deleted directory once rather than every file under it.

use async_trait::async_trait;
use rill_types::{ChangeVersion, RepoPath, RepoPathBuf};
use serde::{Deserialize, Serialize};

use super::ctx::DeriveCtx;
use super::traits::{Derivable, Schedule};
use crate::error::Result;
use crate::util::Bloom;

/// What a version touched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangedPathSet {
    /// Changed paths in path order.
    paths: Vec<RepoPathBuf>,
    /// `=path` for each changed path and `/dir` for each directory above
    /// one, so a query never needs the list when the answer is no.
    filter: Bloom,
}

impl ChangedPathSet {
    /// The set for these changed paths.
    pub fn new(paths: impl IntoIterator<Item = RepoPathBuf>) -> Self {
        let mut paths: Vec<RepoPathBuf> = paths.into_iter().collect();
        paths.sort();
        paths.dedup();
        let mut filter = Bloom::with_capacity(paths.len() * 4);
        for path in &paths {
            filter.insert(format!("={path}").as_bytes());
            for dir in ancestors(path) {
                filter.insert(format!("/{dir}").as_bytes());
            }
        }
        ChangedPathSet { paths, filter }
    }

    /// The changed paths in path order.
    pub fn paths(&self) -> &[RepoPathBuf] {
        &self.paths
    }

    /// Whether the version changed `path`, something under it, or a
    /// directory or file above it that replaced it.
    pub fn touches(&self, path: &RepoPath) -> bool {
        if path.is_root() {
            return !self.paths.is_empty();
        }
        let maybe = self.filter.may_contain(format!("={path}").as_bytes())
            || self.filter.may_contain(format!("/{path}").as_bytes())
            || ancestors(path).any(|a| self.filter.may_contain(format!("={a}").as_bytes()));
        maybe && self.touches_exactly(path)
    }

    fn touches_exactly(&self, path: &RepoPath) -> bool {
        let has = |p: &RepoPath| self.paths.binary_search_by(|q| q.as_path().cmp(p)).is_ok();
        // Paths under `path` share the prefix `path/`, but other names can
        // sort between `path` and them (`path-x` < `path/a`), so search
        // for the prefix itself.
        let under = format!("{path}/");
        let i = self.paths.partition_point(|q| q.as_str() < under.as_str());
        has(path)
            || self
                .paths
                .get(i)
                .is_some_and(|q| q.as_str().starts_with(&under))
            || ancestors(path).any(has)
    }

    /// Whether two versions touched overlapping paths.
    pub fn overlaps(&self, other: &ChangedPathSet) -> bool {
        let (small, large) = if self.paths.len() <= other.paths.len() {
            (self, other)
        } else {
            (other, self)
        };
        small.paths.iter().any(|p| large.touches(p))
    }
}

/// The directories above `path`, nearest first, without the root.
fn ancestors(path: &RepoPath) -> impl Iterator<Item = &RepoPath> {
    std::iter::successors(Some(path.parent()), |p| (!p.is_root()).then(|| p.parent()))
        .filter(|p| !p.is_root())
}

/// [`ChangedPathSet`] of every version.
pub struct ChangedPaths;

#[async_trait]
impl Derivable for ChangedPaths {
    const KIND: &'static str = "changed";
    const VERSION: u8 = 1;
    const NEEDS_PARENTS: bool = false;
    const SCHEDULE: Schedule = Schedule::LandCritical;
    type Value = ChangedPathSet;

    async fn derive(
        _: &DeriveCtx,
        cv: &ChangeVersion,
        _: Vec<ChangedPathSet>,
    ) -> Result<ChangedPathSet> {
        Ok(ChangedPathSet::new(
            cv.file_changes
                .keys()
                .chain(cv.subtree_changes.keys())
                .cloned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(paths: &[&str]) -> ChangedPathSet {
        ChangedPathSet::new(paths.iter().map(|p| RepoPathBuf::new(p).unwrap()))
    }

    fn p(s: &str) -> RepoPathBuf {
        RepoPathBuf::new(s).unwrap()
    }

    #[test]
    fn touches_paths_their_directories_and_what_they_replace() {
        let s = set(&["src/billing-x", "src/billing/tax.rs", "docs"]);
        assert!(s.touches(&p("src/billing/tax.rs")));
        assert!(s.touches(&p("src/billing")));
        assert!(s.touches(&p("src")));
        assert!(s.touches(&p("docs/guide/intro.md")));
        assert!(!s.touches(&p("src/billing/vat.rs")));
        assert!(!s.touches(&p("src/bill")));
        assert!(!s.touches(&p("doc")));
    }

    #[test]
    fn overlap_is_symmetric_and_prefix_aware() {
        let a = set(&["src/a.rs", "lib/x"]);
        let b = set(&["src"]);
        let c = set(&["tests/a.rs"]);
        assert!(a.overlaps(&b) && b.overlaps(&a));
        assert!(!a.overlaps(&c) && !c.overlaps(&a));
    }
}
