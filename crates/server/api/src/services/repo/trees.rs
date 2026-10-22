//! Tree, blob and comparison RPCs.

use std::pin::Pin;

use bytes::Bytes;
use futures::{Stream, StreamExt};
use rill_core::{content_parts, read_content, store_content_stream};
use rill_proto::convert::{hash_from_proto, hash_to_proto};
use rill_proto::rill::v1 as pb;
use rill_store::ObjectStore;
use rill_types::{ContentRef, ObjectKind, RepoPathBuf};

use super::RepoSvc;
use crate::grants::{self, GrantBuilder};
use crate::services::call::next_message;
use crate::services::common::{hash, open, path, resolve_root};
use crate::services::diff::{file_diffs, DiffOptions};
use crate::services::Call;
use connectrpc::{ConnectError, InboundStream};
use rill_runtime::error::{Error, Result};

/// Size of each `GetBlob` chunk.
pub const BLOB_CHUNK: usize = 256 * 1024;

/// Server stream of blob chunks.
pub type BlobStream =
    Pin<Box<dyn Stream<Item = std::result::Result<pb::BlobChunk, ConnectError>> + Send>>;

impl RepoSvc {
    pub(super) async fn get_tree_impl(
        &self,
        request: Call<pb::GetTreeRequest>,
    ) -> Result<pb::GetTreeResponse> {
        let req = request.get_ref();
        let opened = open(&self.state, &request, &req.repo).await?;
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

    pub(super) async fn list_dir_impl(
        &self,
        request: Call<pb::ListDirRequest>,
    ) -> Result<pb::ListDirResponse> {
        let req = request.get_ref();
        let opened = open(&self.state, &request, &req.repo).await?;
        let path = path("path", &req.path)?;
        let filter = opened.permit().read_any().await?;
        if !filter.sees_into(&path) {
            opened.permit().read(&path).await?;
        }
        let ctx = &opened.repo;
        let resolved = resolve_root(ctx, req.root.as_option()).await?;
        let dir = ctx
            .trees
            .dir_hash(resolved.root, &path)
            .await?
            .ok_or_else(|| Error::not_found(format!("directory {path:?}")))?;
        let mut grant = GrantBuilder::new(filter.clone());
        grant.tree(dir, &path);
        let mut entries = Vec::new();
        for (name, kind) in ctx.trees.list_dir(resolved.root, &path).await? {
            let Ok(child) = path.join(&name) else {
                continue;
            };
            let kind = match kind {
                rill_types::EntryKind::Dir { hash, .. } if filter.allows(&child) => {
                    grant.tree(hash, &child);
                    kind
                }
                // On the way to something readable: named, but its
                // summary would count what the caller cannot see.
                rill_types::EntryKind::Dir { hash, .. } if filter.sees_into(&child) => {
                    rill_types::EntryKind::Dir {
                        hash,
                        summary: Default::default(),
                    }
                }
                rill_types::EntryKind::File { ref content, .. } if filter.allows(&child) => {
                    grant.content(content, &child);
                    kind
                }
                _ => continue,
            };
            entries.push(pb::NamedTreeEntry {
                name,
                entry: (pb::TreeEntry::from(rill_types::TreeEntry { kind })).into(),
            });
        }
        Ok(pb::ListDirResponse {
            tree_hash: hash_to_proto(&dir),
            entries,
            read_grant: grant.sign(&opened)?,
        })
    }

    pub(super) async fn resolve_path_impl(
        &self,
        request: Call<pb::ResolvePathRequest>,
    ) -> Result<pb::ResolvePathResponse> {
        let req = request.get_ref();
        let opened = open(&self.state, &request, &req.repo).await?;
        let path = path("path", &req.path)?;
        opened.permit().read(&path).await?;
        let resolved = resolve_root(&opened.repo, req.root.as_option()).await?;
        let entry = opened.repo.trees.lookup(resolved.root, &path).await?;
        let mut grant = GrantBuilder::new(opened.permit().read_filter().await?);
        if let Some(kind) = &entry {
            grant_entry(&mut grant, kind, &path);
        }
        Ok(pb::ResolvePathResponse {
            entry: (entry.map(|kind| pb::TreeEntry::from(rill_types::TreeEntry { kind }))).into(),
            read_grant: grant.sign(&opened)?,
        })
    }

    pub(super) async fn get_blob_impl(
        &self,
        request: Call<pb::GetBlobRequest>,
    ) -> Result<BlobStream> {
        let req = request.get_ref();
        let opened = open(&self.state, &request, &req.repo).await?;
        let content = ContentRef::try_from(
            req.content
                .clone()
                .ok_or_else(|| Error::invalid("content is required"))?,
        )?;
        match &content {
            ContentRef::Inline { .. } => {}
            ContentRef::Blob { hash, .. } => {
                grants::require(&opened, Some(&req.grant), ObjectKind::Blob, hash)?;
            }
            ContentRef::Chunked { hash, .. } => {
                grants::require(&opened, Some(&req.grant), ObjectKind::BlobManifest, hash)?;
            }
        }
        let objects = opened.repo.repo.objects().clone();
        let offset = req.offset.unwrap_or(0);
        let length = req.length.unwrap_or(u64::MAX);
        // Each stored chunk is sent as it is read, split to the wire size,
        // so a large file is never held whole.
        let parts = content_parts(objects, &content, offset, length).await?;
        let chunks = parts.flat_map(|part| {
            let pieces: Vec<std::result::Result<pb::BlobChunk, ConnectError>> = match part {
                Ok(part) => (0..part.len())
                    .step_by(BLOB_CHUNK)
                    .map(|start| {
                        Ok(pb::BlobChunk {
                            data: part.slice(start..(start + BLOB_CHUNK).min(part.len())),
                        })
                    })
                    .collect(),
                Err(e) => vec![Err(ConnectError::from(Error::from(e)))],
            };
            futures::stream::iter(pieces)
        });
        Ok(Box::pin(chunks))
    }

    pub(super) async fn put_blob_impl(
        &self,
        request: Call<InboundStream<pb::PutBlobRequest>>,
    ) -> Result<pb::PutBlobResponse> {
        let ctx = crate::auth::caller(&request)?;
        let mut stream = request.into_inner();
        // The header comes first, so permission is checked before any data
        // is accepted.
        let header = match next_message(&mut stream).await? {
            Some(pb::PutBlobRequest {
                msg: Some(pb::put_blob_request::Msg::Header(h)),
            }) => h,
            _ => return Err(Error::invalid("PutBlob must start with a header")),
        };
        let repo = self.state.repos.resolve(&header.repo).await?;
        let opened = crate::services::common::opened(&self.state, ctx, repo);
        // The commit naming this content checks its path.
        opened.permit().write_any().await?;

        let parts = futures::stream::unfold(stream, |mut stream| async move {
            match next_message(&mut stream).await {
                Ok(Some(pb::PutBlobRequest {
                    msg: Some(pb::put_blob_request::Msg::Data(data)),
                })) => Some((Ok(data), stream)),
                Ok(Some(pb::PutBlobRequest {
                    msg: Some(pb::put_blob_request::Msg::Header(_)),
                })) => Some((
                    Err(rill_core::Error::Upload("duplicate PutBlob header".into())),
                    stream,
                )),
                Ok(Some(pb::PutBlobRequest { msg: None })) => Some((Ok(Bytes::new()), stream)),
                Ok(None) => None,
                Err(status) => Some((
                    Err(rill_core::Error::Upload(status.message.unwrap_or_default())),
                    stream,
                )),
            }
        });
        let stored =
            store_content_stream(opened.repo.repo.objects().as_ref(), Box::pin(parts)).await?;
        if let Some(size) = header.size {
            if size != stored.size {
                return Err(Error::invalid(format!(
                    "PutBlob header announced {size} bytes but {} arrived",
                    stored.size
                )));
            }
        }
        // The expected hash is the hash of the bytes as a blob, whatever
        // storage form (inline, blob, chunked) the server picks.
        if let Some(expected) = &header.expected_hash {
            let expected = hash_from_proto("expected_hash", expected)?;
            if expected != stored.blob_hash {
                return Err(Error::invalid(format!(
                    "content hash mismatch: expected {expected}, got {}",
                    stored.blob_hash
                )));
            }
        }
        Ok(pb::PutBlobResponse {
            content: pb::ContentRef::from(stored.content).into(),
        })
    }

    pub(super) async fn has_objects_impl(
        &self,
        request: Call<pb::HasObjectsRequest>,
    ) -> Result<pb::HasObjectsResponse> {
        let req = request.get_ref();
        let opened = open(&self.state, &request, &req.repo).await?;
        // Existence answers only to someone about to upload.
        opened.permit().write_any().await?;
        let mut items = Vec::with_capacity(req.objects.len());
        for o in &req.objects {
            let kind = o
                .kind
                .as_known()
                .and_then(|k| ObjectKind::from_u8(k as u8))
                .ok_or_else(|| Error::invalid(format!("unknown object kind {}", o.kind)))?;
            items.push((kind, hash_from_proto("objects.hash", &o.hash)?));
        }
        let present = opened.repo.repo.objects().has_many(&items).await?;
        Ok(pb::HasObjectsResponse { present })
    }

    pub(super) async fn diff_impl(
        &self,
        request: Call<pb::DiffRequest>,
    ) -> Result<pb::DiffResponse> {
        let req = request.get_ref();
        let opened = open(&self.state, &request, &req.repo).await?;
        let filter = opened.permit().read_any().await?;
        let ctx = &opened.repo;
        let base = resolve_root(ctx, req.base.as_option()).await?;
        let target = resolve_root(ctx, req.target.as_option()).await?;
        let target_cv = match target.version {
            Some(v) => Some(ctx.derive.change(&v).await?),
            None => None,
        };
        let options = DiffOptions::new(
            &req.paths,
            req.format,
            req.context_lines,
            req.limit,
            filter.clone(),
        )?;
        let (files, truncated) = file_diffs(
            ctx,
            Some(base.root),
            Some(target.root),
            target_cv.as_ref(),
            &options,
        )
        .await?;
        let mut grant = GrantBuilder::new(filter);
        grant_diffs(&mut grant, &files);
        Ok(pb::DiffResponse {
            files,
            truncated,
            read_grant: grant.sign(&opened)?,
        })
    }

    pub(super) async fn merge_base_impl(
        &self,
        request: Call<pb::MergeBaseRequest>,
    ) -> Result<pb::MergeBaseResponse> {
        let req = request.get_ref();
        let opened = open(&self.state, &request, &req.repo).await?;
        opened.permit().read_any().await?;
        let a = hash("a", &req.a)?;
        let b = hash("b", &req.b)?;
        let base = opened.repo.graph.merge_base(&a, &b).await?;
        Ok(pb::MergeBaseResponse {
            bases: base.map(|h| vec![hash_to_proto(&h)]).unwrap_or_default(),
        })
    }

    pub(super) async fn is_ancestor_impl(
        &self,
        request: Call<pb::IsAncestorRequest>,
    ) -> Result<pb::IsAncestorResponse> {
        let req = request.get_ref();
        let opened = open(&self.state, &request, &req.repo).await?;
        opened.permit().read_any().await?;
        let ancestor = hash("ancestor", &req.ancestor)?;
        let descendant = hash("descendant", &req.descendant)?;
        Ok(pb::IsAncestorResponse {
            is_ancestor: opened
                .repo
                .graph
                .is_ancestor(&ancestor, &descendant)
                .await?,
        })
    }
}

/// Verify that the stored object a content ref points at exists.
pub async fn check_content_exists(objects: &dyn ObjectStore, content: &ContentRef) -> Result<()> {
    if let Some((kind, hash)) = content.stored_hash() {
        if !objects.has(kind, &hash).await? {
            return Err(Error::precondition(format!(
                "referenced {kind} {hash} has not been uploaded"
            )));
        }
    }
    if let ContentRef::Inline { bytes } = content {
        if bytes.len() > MAX_INLINE {
            return Err(Error::invalid(format!(
                "inline content of {} bytes exceeds {MAX_INLINE}; upload it as a blob",
                bytes.len()
            )));
        }
    }
    Ok(())
}

/// Largest inline content accepted in a change version.
pub const MAX_INLINE: usize = 1024 * 1024;

/// Bytes of a content ref, for patching.
pub async fn content_bytes(objects: &dyn ObjectStore, content: &ContentRef) -> Result<Bytes> {
    Ok(read_content(objects, content).await?)
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

/// Grant the content on both sides of each file diff.
pub(crate) fn grant_diffs(grant: &mut GrantBuilder, files: &[pb::FileDiff]) {
    for file in files {
        let Ok(path) = RepoPathBuf::new(file.path.as_str()) else {
            continue;
        };
        for state in [&file.before, &file.after]
            .into_iter()
            .filter_map(|s| s.as_option())
        {
            if let Some(content) = state
                .content
                .clone()
                .into_option()
                .and_then(|c| ContentRef::try_from(c).ok())
            {
                grant.content(&content, &path);
            }
        }
    }
}
