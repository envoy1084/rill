//! Deciding whether a new version is "the same change" as the previous
//! one, so that votes may be carried over Gerrit-style.

use std::collections::BTreeMap;

use rill_core::{diff_trees, RootTree};
use rill_types::{ChangeVersion, ChangeVersionHash, FileKind, ObjectHash, RepoPathBuf, TreeHash};

use crate::error::Result;
use crate::service::ReviewService;

/// How the new version relates to the previous one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionRelation {
    /// Same root tree: only the message, metadata or parents changed
    /// without affecting content.
    SameContent,
    /// Different parent, but the same effect on top of it: every touched
    /// path has the same before/after content in both versions.
    TrivialRebase,
    /// The content changed; votes must be recast.
    Changed,
}

impl VersionRelation {
    /// Whether votes for labels with `copy_on_trivial_rebase` survive.
    pub fn preserves_votes(self) -> bool {
        matches!(
            self,
            VersionRelation::SameContent | VersionRelation::TrivialRebase
        )
    }
}

/// A path's content identity on both sides of a version's diff against its
/// first parent. `FileKind` is included so an executable-bit flip is a
/// real change.
type Effect = BTreeMap<
    RepoPathBuf,
    (
        Option<(ObjectHash, FileKind)>,
        Option<(ObjectHash, FileKind)>,
    ),
>;

impl ReviewService {
    /// Classify `new` relative to `old`.
    pub async fn relate_versions(
        &self,
        old: &ChangeVersionHash,
        new: &ChangeVersionHash,
    ) -> Result<VersionRelation> {
        let old_root = self.derive.get::<RootTree>(old).await?;
        let new_root = self.derive.get::<RootTree>(new).await?;
        if old_root == new_root {
            return Ok(VersionRelation::SameContent);
        }
        let old_cv = self.derive.change(old).await?;
        let new_cv = self.derive.change(new).await?;
        // Conflicts are not part of the tree, so a version that merely
        // records a conflict differently must be treated as changed.
        if old_cv.has_conflicts() || new_cv.has_conflicts() {
            return Ok(VersionRelation::Changed);
        }
        let old_effect = self.effect(&old_cv, old_root).await?;
        let new_effect = self.effect(&new_cv, new_root).await?;
        if old_effect == new_effect {
            Ok(VersionRelation::TrivialRebase)
        } else {
            Ok(VersionRelation::Changed)
        }
    }

    /// The per-path `(before, after)` content identities of a version's
    /// tree diff against its first parent.
    pub(crate) async fn effect(&self, cv: &ChangeVersion, root: TreeHash) -> Result<Effect> {
        let parent_root = match cv.first_parent() {
            Some(p) => Some(self.derive.get::<RootTree>(&p).await?),
            None => None,
        };
        let entries = diff_trees(self.derive.trees(), parent_root, Some(root)).await?;
        Ok(entries
            .into_iter()
            .map(|e| {
                let id = |s: Option<(rill_types::ContentRef, FileKind)>| {
                    s.map(|(c, k)| (c.content_hash(), k))
                };
                (e.path, (id(e.before), id(e.after)))
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::fixture;

    #[tokio::test]
    async fn classifies_message_rebase_and_edit() {
        let fx = fixture().await;
        let (_, base) = fx.root_change("a.txt", "one\n").await;
        let (cid, v1) = fx.child_change(base, "b.txt", "two\n").await;

        // Message-only amend.
        let v2 = fx
            .amend(&cid, v1, base, "b.txt", "two\n", "new message")
            .await;
        assert_eq!(
            fx.service.relate_versions(&v1, &v2).await.unwrap(),
            VersionRelation::SameContent
        );

        // Rebase onto a new base that touches an unrelated file.
        let (_, base2) = fx.child_change(base, "c.txt", "three\n").await;
        let v3 = fx
            .amend(&cid, v2, base2, "b.txt", "two\n", "new message")
            .await;
        assert_eq!(
            fx.service.relate_versions(&v2, &v3).await.unwrap(),
            VersionRelation::TrivialRebase
        );

        // Real edit.
        let v4 = fx
            .amend(&cid, v3, base2, "b.txt", "TWO\n", "new message")
            .await;
        assert_eq!(
            fx.service.relate_versions(&v3, &v4).await.unwrap(),
            VersionRelation::Changed
        );
    }
}
