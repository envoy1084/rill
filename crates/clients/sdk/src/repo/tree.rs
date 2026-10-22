//! Tree queries: directory listings, path resolution and file reads.

use bytes::Bytes;
use rill_proto::convert::{hash_from_proto, hash_to_proto, required};
use rill_proto::rill::v1 as pb;
use rill_types::{EntryKind, RepoPath, Tree, TreeEntry, TreeHash};

use super::RepoClient;
use crate::error::{Error, Result};
use crate::grants::entry_hash;
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
        let grant = self.grants().for_hash(&hash);
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.content();
            let request = pb::GetTreeRequest {
                repo: self.resource.clone(),
                hash: hash_to_proto(&hash),
                grant: grant.clone(),
            };
            async move { svc.get_tree(request).await }
        })
        .await?
        .into_owned();
        let tree: Tree = required("get_tree.tree", resp.tree)?.try_into()?;
        self.grants().record(
            &resp.read_grant,
            tree.entries.values().filter_map(|e| entry_hash(&e.kind)),
        );
        Ok(tree)
    }

    /// Entries of the directory at `path` under `root`, sorted by name.
    pub async fn list_dir(&self, root: &Root, path: &RepoPath) -> Result<Vec<(String, EntryKind)>> {
        Ok(self.list_dir_full(root, path).await?.entries)
    }

    /// [`RepoClient::list_dir`] with the directory's tree hash.
    pub async fn list_dir_full(&self, root: &Root, path: &RepoPath) -> Result<DirListing> {
        let mut entries = Vec::new();
        let mut page_token = String::new();
        let mut tree_hash;
        loop {
            let resp = crate::client::retry_idempotent(|| {
                let svc = self.content();
                let request = pb::ListDirectoryRequest {
                    repo: self.resource.clone(),
                    revision: root.to_string(),
                    path: path.to_string(),
                    page_size: 1000,
                    page_token: page_token.clone(),
                };
                async move { svc.list_directory(request).await }
            })
            .await?
            .into_owned();
            let page = resp
                .entries
                .into_iter()
                .map(|e| {
                    let entry: TreeEntry = required("list_directory.entry", e.entry)?.try_into()?;
                    Ok((e.name, entry.kind))
                })
                .collect::<Result<Vec<(String, EntryKind)>>>()?;
            let hash = hash_from_proto("list_directory.tree_hash", &resp.tree_hash)?;
            self.grants().record(
                &resp.read_grant,
                page.iter()
                    .filter_map(|(_, kind)| entry_hash(kind))
                    .chain([hash]),
            );
            tree_hash = hash;
            entries.extend(page);
            if resp.next_page_token.is_empty() {
                break;
            }
            page_token = resp.next_page_token;
        }
        Ok(DirListing { tree_hash, entries })
    }

    /// The entry at `path` under `root`, or `None` if it does not exist.
    pub async fn resolve_path(&self, root: &Root, path: &RepoPath) -> Result<Option<EntryKind>> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.content();
            let request = pb::StatRequest {
                repo: self.resource.clone(),
                revision: root.to_string(),
                path: path.to_string(),
            };
            async move { svc.stat(request).await }
        })
        .await?
        .into_owned();
        match resp.entry.into_option() {
            None => Ok(None),
            Some(e) => {
                let kind = TreeEntry::try_from(e)?.kind;
                self.grants().record(&resp.read_grant, entry_hash(&kind));
                Ok(Some(kind))
            }
        }
    }

    /// The full content of the file at `path` under `root`.
    ///
    /// Fails with `NotFound` if the path is absent or a directory.
    pub async fn read_file(&self, root: &Root, path: &RepoPath) -> Result<Bytes> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.content();
            let request = pb::ReadFileRequest {
                repo: self.resource.clone(),
                revision: root.to_string(),
                path: path.to_string(),
                max_bytes: 0,
            };
            async move { svc.read_file(request).await }
        })
        .await?
        .into_owned();
        let entry: TreeEntry = required("read_file.entry", resp.entry)?.try_into()?;
        self.grants()
            .record(&resp.read_grant, entry_hash(&entry.kind));
        match entry.kind {
            // Too large to come back inline: stream it.
            EntryKind::File { content, .. } if resp.truncated => self.get_blob(&content).await,
            EntryKind::File { .. } => Ok(resp.data),
            EntryKind::Dir { .. } => Err(Error::status(
                connectrpc::ErrorCode::FailedPrecondition,
                format!("{path} is a directory"),
            )),
        }
    }
}
