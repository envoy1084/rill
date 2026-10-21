//! A predicate over repository paths: which ones a caller may see.

use rill_types::{RepoPath, RepoPathBuf};

/// The paths at or below a set of roots. Built once per request and applied
/// to every candidate path (listings, diffs, search hits, event payloads)
/// without touching the store again. Relations, token paths and agent
/// scopes are all prefix sets, so their intersection is one too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathFilter {
    /// `None` allows everything; otherwise the minimal set of roots.
    roots: Option<Vec<RepoPathBuf>>,
}

impl PathFilter {
    /// Allow everything.
    pub fn allow_all() -> Self {
        PathFilter { roots: None }
    }

    /// Deny everything.
    pub fn deny_all() -> Self {
        PathFilter {
            roots: Some(Vec::new()),
        }
    }

    /// Allow paths at or below any of `roots`. An empty list denies all; a
    /// list containing the root allows all.
    pub fn under(roots: impl IntoIterator<Item = RepoPathBuf>) -> Self {
        let mut roots: Vec<RepoPathBuf> = roots.into_iter().collect();
        if roots.iter().any(|r| r.is_root()) {
            return Self::allow_all();
        }
        roots.sort();
        roots.dedup();
        let all = roots.clone();
        roots.retain(|r| !all.iter().any(|o| o != r && r.starts_with(o)));
        PathFilter { roots: Some(roots) }
    }

    /// Paths both filters allow.
    pub fn and(self, other: PathFilter) -> Self {
        match (self.roots, other.roots) {
            (None, roots) | (roots, None) => PathFilter { roots },
            (Some(a), Some(b)) => {
                let inside =
                    |p: &RepoPathBuf, set: &[RepoPathBuf]| set.iter().any(|r| p.starts_with(r));
                let both = a
                    .iter()
                    .filter(|p| inside(p, &b))
                    .chain(b.iter().filter(|p| inside(p, &a)))
                    .cloned()
                    .collect::<Vec<_>>();
                PathFilter::under(both).nonempty_or_none()
            }
        }
    }

    fn nonempty_or_none(self) -> Self {
        match self.roots {
            Some(r) if r.is_empty() => Self::deny_all(),
            roots => PathFilter { roots },
        }
    }

    /// Whether `path` and everything below it pass.
    pub fn allows(&self, path: &RepoPath) -> bool {
        match &self.roots {
            None => true,
            Some(roots) => roots.iter().any(|r| path.starts_with(r)),
        }
    }

    /// Whether `dir` passes or leads to something that does: a directory
    /// that may be listed, showing only the way there.
    pub fn sees_into(&self, dir: &RepoPath) -> bool {
        match &self.roots {
            None => true,
            Some(roots) => roots
                .iter()
                .any(|r| r.starts_with(dir) || dir.starts_with(r)),
        }
    }

    /// Whether nothing passes.
    pub fn is_empty(&self) -> bool {
        self.roots.as_ref().is_some_and(|r| r.is_empty())
    }

    /// Whether everything passes.
    pub fn is_all(&self) -> bool {
        self.roots.is_none()
    }

    /// Keep only the paths that pass.
    pub fn retain<P: AsRef<RepoPath>>(&self, paths: &mut Vec<P>) {
        paths.retain(|p| self.allows(p.as_ref()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> RepoPathBuf {
        RepoPathBuf::new(s).unwrap()
    }

    #[test]
    fn intersections_are_exact_prefix_sets() {
        let relations = PathFilter::under([p("src"), p("docs/public")]);
        let token = PathFilter::under([p("src/billing"), p("docs")]);
        let both = relations.and(token);
        assert_eq!(
            both,
            PathFilter::under([p("src/billing"), p("docs/public")])
        );
        assert!(both.allows(&p("src/billing/x")));
        assert!(!both.allows(&p("src/auth")));
        assert!(!both.allows(&p("docs/private")));
        assert!(PathFilter::under([p("a")])
            .and(PathFilter::under([p("b")]))
            .is_empty());
        assert!(PathFilter::allow_all()
            .and(PathFilter::allow_all())
            .is_all());
        assert_eq!(
            PathFilter::allow_all().and(PathFilter::under([p("a")])),
            PathFilter::under([p("a")])
        );
    }

    #[test]
    fn listing_shows_the_way_to_readable_directories() {
        let f = PathFilter::under([p("src/billing")]);
        assert!(f.sees_into(&RepoPathBuf::root()));
        assert!(f.sees_into(&p("src")));
        assert!(f.sees_into(&p("src/billing/deep")));
        assert!(!f.sees_into(&p("src/auth")));
        assert!(!f.allows(&p("src")), "seeing into is not reading");
        assert!(!PathFilter::deny_all().sees_into(&RepoPathBuf::root()));
    }
}
