//! Files, directories and trees at a revision.

use rill_core::read_content;
use rill_proto::convert::hash_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::{EntryKind, ObjectKind, RepoPath, RepoPathBuf, TreeEntry};

use super::ContentSvc;
use crate::grants::{self, GrantBuilder};
use crate::listing::Page;
use crate::services::common::{hash, open_repo, path, Opened};
use crate::services::{revision, Call};
use rill_runtime::error::{Error, Result};

/// The most content one ReadFile returns.
pub const MAX_READ_BYTES: u64 = 4 << 20;
/// The most content one BatchReadFiles returns, over all its files.
pub const MAX_BATCH_BYTES: u64 = 16 << 20;
/// The most paths one BatchReadFiles reads.
pub const MAX_BATCH_FILES: usize = 1000;

impl ContentSvc {
    pub(super) async fn read_file_impl(
        &self,
        request: Call<pb::ReadFileRequest>,
    ) -> Result<pb::ReadFileResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.repo).await?;
        let path = path("path", &req.path)?;
        opened.permit().read(&path).await?;
        let root = revision::resolve(&opened.repo, &req.revision).await?.root;
        let max = limit(req.max_bytes, MAX_READ_BYTES);
        let mut budget = max;
        read_one(&opened, root, &path, max, &mut budget)
            .await?
            .ok_or_else(|| Error::not_found(format!("file {path}")))
    }

    pub(super) async fn batch_read_files_impl(
        &self,
        request: Call<pb::BatchReadFilesRequest>,
    ) -> Result<pb::BatchReadFilesResponse> {
        let req = request.get_ref();
        if req.paths.len() > MAX_BATCH_FILES {
            return Err(Error::invalid(format!(
                "at most {MAX_BATCH_FILES} paths per batch"
            )));
        }
        let opened = open_repo(&self.state, &request, &req.repo).await?;
        let filter = opened.permit().read_any().await?;
        let root = revision::resolve(&opened.repo, &req.revision).await?.root;
        let max = limit(req.max_bytes, MAX_READ_BYTES);
        let mut budget = MAX_BATCH_BYTES;
        let mut files = Vec::with_capacity(req.paths.len());
        for raw in &req.paths {
            let path = path("paths", raw)?;
            let file = if filter.allows(&path) {
                read_one(&opened, root, &path, max, &mut budget).await?
            } else {
                None
            };
            files.push(file.unwrap_or_default());
        }
        Ok(pb::BatchReadFilesResponse { files })
    }

    pub(super) async fn stat_impl(
        &self,
        request: Call<pb::StatRequest>,
    ) -> Result<pb::StatResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.repo).await?;
        let path = path("path", &req.path)?;
        opened.permit().read(&path).await?;
        let root = revision::resolve(&opened.repo, &req.revision).await?.root;
        let entry = opened.repo.trees.lookup(root, &path).await?;
        let mut grant = GrantBuilder::new(opened.permit().read_filter().await?);
        if let Some(kind) = &entry {
            grant_entry(&mut grant, kind, &path);
        }
        Ok(pb::StatResponse {
            entry: entry
                .map(|kind| pb::TreeEntry::from(TreeEntry { kind }))
                .into(),
            read_grant: grant.sign(&opened)?,
        })
    }

    pub(super) async fn list_directory_impl(
        &self,
        request: Call<pb::ListDirectoryRequest>,
    ) -> Result<pb::ListDirectoryResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.repo).await?;
        let path = path("path", &req.path)?;
        let page = Page::new(
            req.page_size,
            &req.page_token,
            &[&req.repo, &req.revision, &req.path],
        )?;
        let filter = opened.permit().read_any().await?;
        if !filter.sees_into(&path) {
            opened.permit().read(&path).await?;
        }
        let ctx = &opened.repo;
        let root = revision::resolve(ctx, &req.revision).await?.root;
        let dir = ctx
            .trees
            .dir_hash(root, &path)
            .await?
            .ok_or_else(|| Error::not_found(format!("directory {path:?}")))?;
        let mut grant = GrantBuilder::new(filter.clone());
        grant.tree(dir, &path);
        let mut visible = Vec::new();
        for (name, kind) in ctx.trees.list_dir(root, &path).await? {
            let Ok(child) = path.join(&name) else {
                continue;
            };
            let kind = match kind {
                EntryKind::Dir { .. } if filter.allows(&child) => kind,
                // On the way to something readable: named, but its
                // summary would count what the caller cannot see.
                EntryKind::Dir { hash, .. } if filter.sees_into(&child) => EntryKind::Dir {
                    hash,
                    summary: Default::default(),
                },
                EntryKind::File { .. } if filter.allows(&child) => kind,
                _ => continue,
            };
            visible.push((name, child, kind));
        }
        let (entries, next_page_token) = page.of(visible)?;
        let entries = entries
            .into_iter()
            .map(|(name, child, kind)| {
                if filter.allows(&child) {
                    grant_entry(&mut grant, &kind, &child);
                }
                pb::NamedTreeEntry {
                    name,
                    entry: pb::TreeEntry::from(TreeEntry { kind }).into(),
                }
            })
            .collect();
        Ok(pb::ListDirectoryResponse {
            tree_hash: hash_to_proto(&dir),
            entries,
            next_page_token,
            read_grant: grant.sign(&opened)?,
        })
    }

    pub(super) async fn get_tree_impl(
        &self,
        request: Call<pb::GetTreeRequest>,
    ) -> Result<pb::GetTreeResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.repo).await?;
        let hash = hash("hash", &req.hash)?;
        // A tree is granted only to callers who may read all of it, so
        // its entries need no filtering; they are granted in turn.
        let at = grants::require(&opened, Some(&req.grant), ObjectKind::Tree, &hash)?
            .unwrap_or_else(RepoPathBuf::root);
        let tree = opened.repo.trees.get_tree(&hash).await?;
        let mut grant = GrantBuilder::new(opened.permit().read_any().await?);
        for (name, entry) in &tree.entries {
            if let Ok(child) = at.join(name) {
                grant_entry(&mut grant, &entry.kind, &child);
            }
        }
        Ok(pb::GetTreeResponse {
            tree: pb::Tree::from(&*tree).into(),
            read_grant: grant.sign(&opened)?,
        })
    }
}

/// A client's byte limit: 0 means `max`, and nothing above it.
fn limit(requested: u64, max: u64) -> u64 {
    if requested == 0 {
        max
    } else {
        requested.min(max)
    }
}

/// The file at `path`, with its content when it fits in `max` and what is
/// left of `budget`; `None` when nothing is there. A directory is refused.
async fn read_one(
    opened: &Opened,
    root: rill_types::TreeHash,
    path: &RepoPath,
    max: u64,
    budget: &mut u64,
) -> Result<Option<pb::ReadFileResponse>> {
    let Some(kind) = opened.repo.trees.lookup(root, path).await? else {
        return Ok(None);
    };
    let EntryKind::File { content, .. } = &kind else {
        return Err(Error::invalid(format!("{path} is a directory")));
    };
    let size = content.size();
    let fits = size <= max && size <= *budget;
    let data = if fits {
        *budget -= size;
        read_content(opened.repo.repo.objects().as_ref(), content).await?
    } else {
        Default::default()
    };
    let mut grant = GrantBuilder::new(opened.permit().read_filter().await?);
    grant.content(content, path);
    Ok(Some(pb::ReadFileResponse {
        entry: pb::TreeEntry::from(TreeEntry { kind: kind.clone() }).into(),
        data,
        truncated: !fits,
        read_grant: grant.sign(opened)?,
    }))
}

/// Grant what a tree entry at `path` names.
fn grant_entry(
    grant: &mut GrantBuilder,
    kind: &rill_types::EntryKind,
    path: &rill_types::RepoPath,
) {
    match kind {
        rill_types::EntryKind::File { content, .. } => grant.content(content, path),
        rill_types::EntryKind::Dir { hash, .. } => grant.tree(*hash, path),
    }
}
