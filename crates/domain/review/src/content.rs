//! Reading file content through a version's root tree.

use bytes::Bytes;
use rill_core::diff::file_state;
use rill_core::{read_content, RootTree};
use rill_types::{ChangeVersionHash, RepoPath, TreeHash};

use crate::error::Result;
use crate::service::ReviewService;

impl ReviewService {
    /// Root tree of a version.
    pub(crate) async fn root_of(&self, version: &ChangeVersionHash) -> Result<TreeHash> {
        Ok(self.derive.get::<RootTree>(version).await?)
    }

    /// Bytes of the file at `path` under `root`, or `None` when absent.
    pub(crate) async fn file_at(&self, root: TreeHash, path: &RepoPath) -> Result<Option<Bytes>> {
        match file_state(self.derive.trees(), Some(root), path).await? {
            Some((content, _)) => Ok(Some(
                read_content(self.repo.objects().as_ref(), &content).await?,
            )),
            None => Ok(None),
        }
    }
}
