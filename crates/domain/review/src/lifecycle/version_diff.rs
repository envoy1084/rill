//! What the author changed between two versions of a change, with the
//! noise of rebasing filtered out.
//!
//! A path whose own patch (its diff against each version's parent) is the
//! same in both versions only moved with the base: it is listed as
//! rebased, not diffed. Every other path the author touched in either
//! version is diffed from its content in the old version to its content in
//! the new one.

use bytes::Bytes;
use rill_core::{diff_lines, Hunk, LineTag, RootTree};
use rill_types::{ChangeVersionHash, RepoPathBuf};

use crate::error::Result;
use crate::service::ReviewService;

/// One path the author changed between the versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterdiffFile {
    pub path: RepoPathBuf,
    /// Its content in the old and new version; `None` when absent.
    pub old: Option<Bytes>,
    pub new: Option<Bytes>,
    /// Line hunks old → new, empty when either side is absent.
    pub hunks: Vec<Hunk>,
}

/// The author's changes between two versions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Interdiff {
    pub files: Vec<InterdiffFile>,
    /// Paths whose patch is unchanged; they differ only by the base.
    pub rebased: Vec<RepoPathBuf>,
}

/// The lines a patch adds and removes, without positions.
fn patch_lines(before: &[u8], after: &[u8]) -> Vec<(bool, Bytes)> {
    diff_lines(before, after)
        .into_iter()
        .flat_map(|h| h.lines)
        .filter_map(|l| match l.tag {
            LineTag::Added => Some((true, l.text)),
            LineTag::Removed => Some((false, l.text)),
            LineTag::Context => None,
        })
        .collect()
}

impl ReviewService {
    /// The author's changes from version `old` to version `new`.
    pub async fn interdiff(
        &self,
        old: &ChangeVersionHash,
        new: &ChangeVersionHash,
    ) -> Result<Interdiff> {
        let old_cv = self.derive.change(old).await?;
        let new_cv = self.derive.change(new).await?;
        let old_root = self.derive.get::<RootTree>(old).await?;
        let new_root = self.derive.get::<RootTree>(new).await?;
        let old_effect = self.effect(&old_cv, old_root).await?;
        let new_effect = self.effect(&new_cv, new_root).await?;
        let parent_root = |cv: &rill_types::ChangeVersion| cv.first_parent();
        let (old_parent, new_parent) = (parent_root(&old_cv), parent_root(&new_cv));
        let mut paths: Vec<&RepoPathBuf> = old_effect.keys().chain(new_effect.keys()).collect();
        paths.sort();
        paths.dedup();
        let mut out = Interdiff::default();
        for path in paths {
            if old_effect.get(path) == new_effect.get(path) {
                continue;
            }
            let old_after = self.file_at(old_root, path).await?;
            let new_after = self.file_at(new_root, path).await?;
            let before = |parent: Option<ChangeVersionHash>| async move {
                match parent {
                    Some(p) => {
                        let root = self.derive.get::<RootTree>(&p).await?;
                        self.file_at(root, path).await
                    }
                    None => Ok(None),
                }
            };
            let old_before = before(old_parent).await?;
            let new_before = before(new_parent).await?;
            let same_patch = match (&old_before, &old_after, &new_before, &new_after) {
                (Some(ob), Some(oa), Some(nb), Some(na)) => {
                    patch_lines(ob, oa) == patch_lines(nb, na)
                }
                _ => false,
            };
            if same_patch {
                out.rebased.push(path.clone());
                continue;
            }
            let hunks = match (&old_after, &new_after) {
                (Some(o), Some(n)) => diff_lines(o, n),
                (None, Some(n)) => diff_lines(b"", n),
                (Some(o), None) => diff_lines(o, b""),
                (None, None) => Vec::new(),
            };
            if old_after == new_after {
                out.rebased.push(path.clone());
                continue;
            }
            out.files.push(InterdiffFile {
                path: path.clone(),
                old: old_after,
                new: new_after,
                hunks,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::fixture;

    #[tokio::test]
    async fn rebase_noise_is_filtered_and_real_edits_shown() {
        let fx = fixture().await;
        let (_, base) = fx.root_change("a.txt", "one\ntwo\n").await;
        let (cid, v1) = fx.child_change(base, "b.txt", "x\n").await;
        // Trunk moves: a.txt changes underneath.
        let (_, base2) = fx.child_change(base, "a.txt", "one\nTWO\n").await;
        let v2 = fx.amend(&cid, v1, base2, "b.txt", "x\n", "same").await;
        let d = fx.service.interdiff(&v1, &v2).await.unwrap();
        assert!(d.files.is_empty(), "{:?}", d.files);
        let v3 = fx.amend(&cid, v2, base2, "b.txt", "x\ny\n", "edit").await;
        let d = fx.service.interdiff(&v2, &v3).await.unwrap();
        assert_eq!(d.files.len(), 1);
        assert_eq!(d.files[0].path.as_str(), "b.txt");
        assert_eq!(d.files[0].hunks.len(), 1);
    }
}
