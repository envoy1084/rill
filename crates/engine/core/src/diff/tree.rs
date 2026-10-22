//! Diffing two trees by recursing only into subtrees whose hashes differ.

use rill_types::{ContentRef, EntryKind, FileKind, RepoPath, RepoPathBuf, TreeHash};

use crate::error::Result;
use crate::tree::TreeOps;

/// A file whose state differs between two trees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeDiffEntry {
    pub path: RepoPathBuf,
    /// State in the first tree; `None` when absent.
    pub before: Option<(ContentRef, FileKind)>,
    /// State in the second tree; `None` when absent.
    pub after: Option<(ContentRef, FileKind)>,
}

impl TreeDiffEntry {
    /// True for a path that did not exist before.
    pub fn is_added(&self) -> bool {
        self.before.is_none() && self.after.is_some()
    }

    /// True for a path that no longer exists.
    pub fn is_deleted(&self) -> bool {
        self.before.is_some() && self.after.is_none()
    }
}

/// All file differences between `before` and `after`, in path order.
///
/// Either side may be `None` for "no tree at all" (the parent of a root
/// change). Subtrees with equal hashes are skipped without being loaded,
/// so the cost is proportional to the changed region, not the repository.
pub async fn diff_trees(
    ops: &TreeOps,
    before: Option<TreeHash>,
    after: Option<TreeHash>,
) -> Result<Vec<TreeDiffEntry>> {
    let mut out = Vec::new();
    if before == after {
        return Ok(out);
    }
    let mut stack = vec![(RepoPathBuf::root(), before, after)];
    while let Some((dir, b, a)) = stack.pop() {
        if b == a {
            continue;
        }
        let (bt, at) = ops.differing(b, a).await?;
        let (b_entries, a_entries) = (&bt.entries, &at.entries);
        let mut names: Vec<&String> = b_entries.keys().chain(a_entries.keys()).collect();
        names.sort();
        names.dedup();
        let mut subdirs = Vec::new();
        for name in names {
            let path = dir.join(name)?;
            let bk = b_entries.get(name).map(|e| &e.kind);
            let ak = a_entries.get(name).map(|e| &e.kind);
            match (bk, ak) {
                (Some(EntryKind::Dir { hash: bh, .. }), Some(EntryKind::Dir { hash: ah, .. })) => {
                    if bh != ah {
                        subdirs.push((path, Some(*bh), Some(*ah)));
                    }
                }
                (bk, ak) => {
                    let (b_file, b_dir) = split(bk);
                    let (a_file, a_dir) = split(ak);
                    if b_file != a_file && (b_file.is_some() || a_file.is_some()) {
                        out.push(TreeDiffEntry {
                            path: path.clone(),
                            before: b_file,
                            after: a_file,
                        });
                    }
                    if b_dir.is_some() || a_dir.is_some() {
                        subdirs.push((path, b_dir, a_dir));
                    }
                }
            }
        }
        stack.extend(subdirs.into_iter().rev());
    }
    out.sort_by(|x, y| x.path.cmp(&y.path));
    Ok(out)
}

type FileState = Option<(ContentRef, FileKind)>;

fn split(kind: Option<&EntryKind>) -> (FileState, Option<TreeHash>) {
    match kind {
        Some(EntryKind::File { content, kind }) => (Some((content.clone(), *kind)), None),
        Some(EntryKind::Dir { hash, .. }) => (None, Some(*hash)),
        None => (None, None),
    }
}

/// Convenience: the file state at `path` in `root`.
pub async fn file_state(
    ops: &TreeOps,
    root: Option<TreeHash>,
    path: &RepoPath,
) -> Result<FileState> {
    let Some(root) = root else {
        return Ok(None);
    };
    Ok(match ops.lookup(root, path).await? {
        Some(EntryKind::File { content, kind }) => Some((content, kind)),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{Edit, TreeBuilder};
    use std::sync::Arc;

    fn p(s: &str) -> RepoPathBuf {
        RepoPathBuf::new(s).unwrap()
    }

    fn set(s: &str) -> Edit {
        Edit::Set(ContentRef::inline(s.as_bytes().to_vec()), FileKind::Regular)
    }

    #[tokio::test]
    async fn reports_adds_modifies_deletes() {
        let ops = TreeOps::new(Arc::new(rill_store::MemoryObjectStore::new()), 16);
        let mut b = TreeBuilder::new();
        b.edit(&p("a/one"), set("1"));
        b.edit(&p("a/two"), set("2"));
        b.edit(&p("keep/same"), set("s"));
        let r1 = b.build(&ops, None).await.unwrap();
        let mut b = TreeBuilder::new();
        b.edit(&p("a/one"), set("1!"));
        b.edit(&p("a/two"), Edit::Delete);
        b.edit(&p("new"), set("n"));
        let r2 = b.build(&ops, Some(r1)).await.unwrap();

        let diff = diff_trees(&ops, Some(r1), Some(r2)).await.unwrap();
        let summary: Vec<_> = diff
            .iter()
            .map(|e| (e.path.as_str(), e.is_added(), e.is_deleted()))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("a/one", false, false),
                ("a/two", false, true),
                ("new", true, false)
            ]
        );
        assert!(diff_trees(&ops, Some(r1), Some(r1))
            .await
            .unwrap()
            .is_empty());
        assert_eq!(diff_trees(&ops, None, Some(r1)).await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn file_to_dir_swap() {
        let ops = TreeOps::new(Arc::new(rill_store::MemoryObjectStore::new()), 16);
        let mut b = TreeBuilder::new();
        b.edit(&p("x"), set("f"));
        let r1 = b.build(&ops, None).await.unwrap();
        let mut b = TreeBuilder::new();
        b.edit(&p("x"), Edit::Delete);
        b.edit(&p("x/y"), set("g"));
        let r2 = b.build(&ops, Some(r1)).await.unwrap();
        let diff = diff_trees(&ops, Some(r1), Some(r2)).await.unwrap();
        let paths: Vec<_> = diff.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, vec!["x", "x/y"]);
        assert!(diff[0].is_deleted());
        assert!(diff[1].is_added());
    }
}
