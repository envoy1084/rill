//! Which directories a sparse profile reaches into.

use rill_types::{RepoPath, SparseProfile};

/// True when some included path may lie under `dir`: the dir is itself
/// included, or it is an ancestor of an include.
pub fn dir_may_contain(sparse: &SparseProfile, dir: &RepoPath) -> bool {
    if sparse.exclude.iter().any(|e| dir.starts_with(e)) {
        return false;
    }
    sparse.include.is_empty()
        || sparse
            .include
            .iter()
            .any(|i| dir.starts_with(i) || i.starts_with(dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_types::RepoPathBuf;

    fn p(s: &str) -> RepoPathBuf {
        RepoPathBuf::new(s).unwrap()
    }

    #[test]
    fn ancestors_of_includes_are_entered_and_excludes_are_not() {
        let sparse = SparseProfile {
            include: vec![p("a/b/c")],
            exclude: vec![p("a/b/c/d")],
        };
        assert!(dir_may_contain(&sparse, &p("a")));
        assert!(dir_may_contain(&sparse, &p("a/b/c/e")));
        assert!(!dir_may_contain(&sparse, &p("x")));
        assert!(!dir_may_contain(&sparse, &p("a/b/c/d")));
    }
}
