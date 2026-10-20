//! Applying a change version to its parent's tree.

use async_trait::async_trait;
use rill_types::{ChangeVersion, ChangeVersionHash, FileChange, SubtreeChange, TreeHash};

use super::builder::{Edit, TreeBuilder};
use super::ops::TreeOps;
use crate::error::{Error, Result};

/// Resolves a change version to its root tree; needed for subtree copies,
/// which reference a directory as of another version.
#[async_trait]
pub trait RootLookup: Send + Sync {
    /// Root tree of `version`.
    async fn root_tree(&self, version: &ChangeVersionHash) -> Result<TreeHash>;
}

impl TreeOps {
    /// Produce the root tree of `cv` given its first parent's root.
    ///
    /// Subtree changes are applied first, then file changes. A `Delete`
    /// whose path is a prefix of `Set` paths replaces a file with a
    /// directory. Conflicts are stored as the first add's state (or absent)
    /// so the tree is always a plain tree; the conflict lives only in the
    /// change.
    pub async fn materialize(
        &self,
        parent_root: Option<TreeHash>,
        cv: &ChangeVersion,
        roots: &dyn RootLookup,
    ) -> Result<TreeHash> {
        let mut builder = TreeBuilder::new();
        for (path, change) in &cv.subtree_changes {
            if path.is_root() {
                return Err(Error::InvalidChange(
                    "subtree change at the repository root".into(),
                ));
            }
            match change {
                SubtreeChange::Delete => builder.edit(path, Edit::Delete),
                SubtreeChange::Copy {
                    from_path,
                    from_version,
                } => {
                    let from_root = roots.root_tree(from_version).await?;
                    let hash = self
                        .dir_hash(from_root, from_path)
                        .await?
                        .ok_or_else(|| Error::NotADirectory(from_path.clone()))?;
                    builder.edit(path, Edit::Dir(hash));
                }
            }
        }
        for (path, change) in &cv.file_changes {
            if path.is_root() {
                return Err(Error::InvalidChange(
                    "file change at the repository root".into(),
                ));
            }
            let edit = match change {
                FileChange::Set { content, kind, .. } => Edit::Set(content.clone(), *kind),
                FileChange::Delete => Edit::Delete,
                FileChange::Conflict(terms) => match terms.first() {
                    Some((content, kind)) => Edit::Set(content.clone(), *kind),
                    None => Edit::Delete,
                },
            };
            builder.edit(path, edit);
        }
        builder.build(self, parent_root).await
    }
}

/// A [`RootLookup`] that knows no versions; for changes without subtree
/// copies.
pub struct NoRoots;

#[async_trait]
impl RootLookup for NoRoots {
    async fn root_tree(&self, version: &ChangeVersionHash) -> Result<TreeHash> {
        Err(Error::ChangeNotFound(*version))
    }
}
