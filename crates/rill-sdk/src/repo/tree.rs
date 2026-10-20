//! Tree queries: directory listings, path resolution and file reads.

use bytes::Bytes;
use rill_proto::convert::{hash_from_proto, hash_to_proto, required};
use rill_proto::rill::v1 as pb;
use rill_types::{EntryKind, RepoPath, Tree, TreeEntry, TreeHash};

use super::RepoClient;
use crate::error::{Error, Result};
use crate::types::Root;

/// A directory listing with the hash of the listed tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirListing {
    /// Hash of the directory's tree object.
    pub tree_hash: TreeHash,
    /// Entries sorted by name.
    pub entries: Vec<(String, EntryKind)>,
}

impl RepoClient {
    /// A tree object by hash.
    pub async fn tree(&self, hash: TreeHash) -> Result<Tree> {
        let resp = self
            .svc()
            .get_tree(pb::GetTreeRequest {
                repo: self.repo.clone(),
                hash: hash_to_proto(&hash),
            })
            .await?
            .into_inner();
        Ok(required("get_tree.tree", resp.tree)?.try_into()?)
    }

    /// Entries of the directory at `path` under `root`, sorted by name.
    pub async fn list_dir(&self, root: &Root, path: &RepoPath) -> Result<Vec<(String, EntryKind)>> {
        Ok(self.list_dir_full(root, path).await?.entries)
    }

    /// [`RepoClient::list_dir`] with the directory's tree hash.
    pub async fn list_dir_full(&self, root: &Root, path: &RepoPath) -> Result<DirListing> {
        let resp = self
            .svc()
            .list_dir(pb::ListDirRequest {
                repo: self.repo.clone(),
                root: Some(root.to_proto()),
                path: path.to_string(),
            })
            .await?
            .into_inner();
        let entries = resp
            .entries
            .into_iter()
            .map(|e| {
                let entry: TreeEntry = required("list_dir.entry", e.entry)?.try_into()?;
                Ok((e.name, entry.kind))
            })
            .collect::<Result<_>>()?;
        Ok(DirListing {
            tree_hash: hash_from_proto("list_dir.tree_hash", &resp.tree_hash)?,
            entries,
        })
    }

    /// The entry at `path` under `root`, or `None` if it does not exist.
    pub async fn resolve_path(&self, root: &Root, path: &RepoPath) -> Result<Option<EntryKind>> {
        let resp = self
            .svc()
            .resolve_path(pb::ResolvePathRequest {
                repo: self.repo.clone(),
                root: Some(root.to_proto()),
                path: path.to_string(),
            })
            .await?
            .into_inner();
        match resp.entry {
            None => Ok(None),
            Some(e) => Ok(Some(TreeEntry::try_from(e)?.kind)),
        }
    }

    /// The full content of the file at `path` under `root`.
    ///
    /// Fails with `NotFound` if the path is absent or a directory.
    pub async fn read_file(&self, root: &Root, path: &RepoPath) -> Result<Bytes> {
        match self.resolve_path(root, path).await? {
            Some(EntryKind::File { content, .. }) => self.get_blob(&content).await,
            Some(EntryKind::Dir { .. }) => Err(Error::Status {
                code: tonic::Code::FailedPrecondition,
                message: format!("{path} is a directory"),
            }),
            None => Err(Error::Status {
                code: tonic::Code::NotFound,
                message: format!("{path} does not exist at {root}"),
            }),
        }
    }
}
