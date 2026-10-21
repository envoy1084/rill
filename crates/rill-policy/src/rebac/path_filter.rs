//! A predicate over repository paths: which ones a caller may see.

use std::sync::Arc;

use rill_types::{RepoPath, RepoPathBuf};

/// Built once per request and applied to every candidate path (listings,
/// diffs, search hits, event payloads) without touching the store again.
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

    /// Paths both filters allow.
    pub fn and(self, other: PathFilter) -> Self {
        PathFilter::new(move |p| self.allows(p) && other.allows(p))
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
