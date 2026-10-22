//! Depth-first file walks with a sparse filter.

use rill_types::{ContentRef, EntryKind, FileKind, RepoPath, RepoPathBuf, SparseProfile, TreeHash};

use super::ops::TreeOps;
use crate::error::Result;

/// One file produced by a walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkEntry {
    pub path: RepoPathBuf,
    pub content: ContentRef,
    pub kind: FileKind,
}

impl TreeOps {
    /// All files under `prefix` in path order, restricted to `sparse`.
    ///
    /// Directories that the sparse profile excludes entirely are not
    /// loaded. The result is collected rather than streamed because the
    /// per-entry cost is a cache hit in the common case; callers paging a
    /// huge repository should pass a narrower `prefix`.
    pub async fn walk(
        &self,
        root: TreeHash,
        prefix: &RepoPath,
        sparse: &SparseProfile,
    ) -> Result<Vec<WalkEntry>> {
        let mut out = Vec::new();
        let Some(start) = self.dir_hash(root, prefix).await? else {
            if let Some(EntryKind::File { content, kind }) = self.lookup(root, prefix).await? {
                if sparse.contains(prefix) {
                    out.push(WalkEntry {
                        path: prefix.to_owned(),
                        content,
                        kind,
                    });
                }
            }
            return Ok(out);
        };
        // Explicit stack, pushed in reverse so names come out sorted.
        let mut stack = vec![(prefix.to_owned(), start)];
        while let Some((dir_path, hash)) = stack.pop() {
            let tree = self.get_tree(&hash).await?;
            let mut subdirs = Vec::new();
            for (name, entry) in &tree.entries {
                let path = dir_path.join(name)?;
                match &entry.kind {
                    EntryKind::File { content, kind } => {
                        if sparse.contains(&path) {
                            out.push(WalkEntry {
                                path,
                                content: content.clone(),
                                kind: *kind,
                            });
                        }
                    }
                    EntryKind::Dir { hash, .. } => {
                        if dir_may_contain(sparse, &path) {
                            subdirs.push((path, *hash));
                        }
                    }
                }
            }
            // Files of this directory were emitted before any subdirectory;
            // that matches BTreeMap order only if names sort that way, so
            // re-sort at the end.
            stack.extend(subdirs.into_iter().rev());
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }
}

/// True unless the directory is excluded, or includes exist and none of
/// them overlaps the directory.
fn dir_may_contain(sparse: &SparseProfile, dir: &RepoPath) -> bool {
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
    use crate::tree::{Edit, TreeBuilder};
    use std::sync::Arc;

    fn p(s: &str) -> RepoPathBuf {
        RepoPathBuf::new(s).unwrap()
    }

    #[tokio::test]
    async fn walks_in_order_with_sparse() {
        let ops = TreeOps::new(Arc::new(rill_store::MemoryObjectStore::new()), 16);
        let mut b = TreeBuilder::new();
        for path in ["b/z", "a/y", "a/x/w", "c"] {
            b.edit(
                &p(path),
                Edit::Set(
                    ContentRef::inline(path.as_bytes().to_vec()),
                    FileKind::Regular,
                ),
            );
        }
        let root = b.build(&ops, None).await.unwrap();
        let all = ops
            .walk(root, &RepoPathBuf::root(), &SparseProfile::full())
            .await
            .unwrap();
        let paths: Vec<_> = all.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, vec!["a/x/w", "a/y", "b/z", "c"]);

        let sparse = SparseProfile {
            include: vec![p("a")],
            exclude: vec![p("a/x")],
        };
        let some = ops.walk(root, &RepoPathBuf::root(), &sparse).await.unwrap();
        let paths: Vec<_> = some.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, vec!["a/y"]);

        let under = ops
            .walk(root, &p("b"), &SparseProfile::full())
            .await
            .unwrap();
        assert_eq!(under.len(), 1);
        assert_eq!(under[0].path.as_str(), "b/z");
    }
}
