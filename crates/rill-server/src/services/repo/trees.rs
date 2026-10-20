//! Tree, blob and comparison RPCs.

use std::pin::Pin;

use bytes::Bytes;
use futures::Stream;
use rill_core::{read_content, store_content};
use rill_proto::convert::{hash_from_proto, hash_to_proto};
use rill_proto::rill::v1 as pb;
use rill_store::ObjectStore;
use rill_types::{ContentRef, ObjectHash, ObjectKind, RepoPathBuf};
use tonic::{Status, Streaming};

use super::RepoSvc;
use crate::error::{Error, Result};
use crate::services::common::{hash, open, path, resolve_root};
use crate::services::diff::{file_diffs, DiffOptions};

/// Size of each `GetBlob` chunk.
pub const BLOB_CHUNK: usize = 256 * 1024;

/// Server stream of blob chunks.
pub type BlobStream =
    Pin<Box<dyn Stream<Item = std::result::Result<pb::BlobChunk, Status>> + Send>>;

impl RepoSvc {
    pub(super) async fn get_tree_impl(
        &self,
        request: tonic::Request<pb::GetTreeRequest>,
    ) -> Result<pb::GetTreeResponse> {
        let req = request.get_ref();
        let opened = open(&self.state, &request, &req.repo).await?;
        // Trees are addressed by hash, not path, so they need repo-wide
        // read: a path-scoped reader cannot be kept out of a subtree hash.
        opened.permit().read(&RepoPathBuf::root()).await?;
        let hash = hash("hash", &req.hash)?;
        let tree = opened.repo.trees.get_tree(&hash).await?;
        Ok(pb::GetTreeResponse {
            tree: Some(pb::Tree::from(&*tree)),
        })
    }

    pub(super) async fn list_dir_impl(
        &self,
        request: tonic::Request<pb::ListDirRequest>,
    ) -> Result<pb::ListDirResponse> {
        let req = request.get_ref();
        let opened = open(&self.state, &request, &req.repo).await?;
        let path = path("path", &req.path)?;
        opened.permit().read(&path).await?;
        let filter = opened.permit().read_filter().await?;
        let ctx = &opened.repo;
        let resolved = resolve_root(ctx, req.root.as_ref()).await?;
        let dir = ctx
            .trees
            .dir_hash(resolved.root, &path)
            .await?
            .ok_or_else(|| Error::not_found(format!("directory {path:?}")))?;
        let entries = ctx
            .trees
            .list_dir(resolved.root, &path)
            .await?
            .into_iter()
            .filter(|(name, _)| path.join(name).map(|p| filter.allows(&p)).unwrap_or(false))
            .map(|(name, kind)| pb::NamedTreeEntry {
                name,
                entry: Some(pb::TreeEntry::from(rill_types::TreeEntry { kind })),
            })
            .collect();
        Ok(pb::ListDirResponse {
            tree_hash: hash_to_proto(&dir),
            entries,
        })
    }

    pub(super) async fn resolve_path_impl(
        &self,
        request: tonic::Request<pb::ResolvePathRequest>,
    ) -> Result<pb::ResolvePathResponse> {
        let req = request.get_ref();
        let opened = open(&self.state, &request, &req.repo).await?;
        let path = path("path", &req.path)?;
        opened.permit().read(&path).await?;
        let resolved = resolve_root(&opened.repo, req.root.as_ref()).await?;
        let entry = opened.repo.trees.lookup(resolved.root, &path).await?;
        Ok(pb::ResolvePathResponse {
            entry: entry.map(|kind| pb::TreeEntry::from(rill_types::TreeEntry { kind })),
        })
    }

    pub(super) async fn get_blob_impl(
        &self,
        request: tonic::Request<pb::GetBlobRequest>,
    ) -> Result<BlobStream> {
        let req = request.get_ref();
        let opened = open(&self.state, &request, &req.repo).await?;
        // Blobs are content-addressed and not tied to a path, so reading
        // one needs repo-wide read (see `GetTree`).
        opened.permit().read(&RepoPathBuf::root()).await?;
        let content = ContentRef::try_from(
            req.content
                .clone()
                .ok_or_else(|| Error::invalid("content is required"))?,
        )?;
        let bytes = read_content(opened.repo.repo.objects().as_ref(), &content).await?;
        let offset = req.offset.unwrap_or(0).min(bytes.len() as u64) as usize;
        let end = match req.length {
            Some(len) => (offset as u64 + len).min(bytes.len() as u64) as usize,
            None => bytes.len(),
        };
        let slice = bytes.slice(offset..end);
        let chunks: Vec<_> = (0..slice.len())
            .step_by(BLOB_CHUNK)
            .map(|start| {
                Ok(pb::BlobChunk {
                    data: slice.slice(start..(start + BLOB_CHUNK).min(slice.len())),
                })
            })
            .collect();
        Ok(Box::pin(futures::stream::iter(chunks)))
    }

    pub(super) async fn put_blob_impl(
        &self,
        request: tonic::Request<Streaming<pb::PutBlobRequest>>,
    ) -> Result<pb::PutBlobResponse> {
        let ctx = crate::auth::caller(&request)?;
        let mut stream = request.into_inner();
        let mut header: Option<pb::PutBlobHeader> = None;
        let mut data = bytes::BytesMut::new();
        while let Some(msg) = stream.message().await? {
            match msg.msg {
                Some(pb::put_blob_request::Msg::Header(h)) => {
                    if header.is_some() {
                        return Err(Error::invalid("duplicate PutBlob header"));
                    }
                    if let Some(size) = h.size {
                        data.reserve(size.min(64 * 1024 * 1024) as usize);
                    }
                    header = Some(h);
                }
                Some(pb::put_blob_request::Msg::Data(chunk)) => {
                    if header.is_none() {
                        return Err(Error::invalid("PutBlob data before header"));
                    }
                    data.extend_from_slice(&chunk);
                }
                None => {}
            }
        }
        let header = header.ok_or_else(|| Error::invalid("PutBlob stream had no header"))?;
        let repo = self.state.repos.resolve(&header.repo).await?;
        let opened = crate::services::common::opened(&self.state, ctx, repo);
        opened.permit().write(&RepoPathBuf::root()).await?;
        let bytes = data.freeze();
        if let Some(size) = header.size {
            if size != bytes.len() as u64 {
                return Err(Error::invalid(format!(
                    "PutBlob header announced {size} bytes but {} arrived",
                    bytes.len()
                )));
            }
        }
        // The expected hash is the hash of the bytes as a blob, whatever
        // storage form (inline, blob, chunked) the server picks.
        if let Some(expected) = &header.expected_hash {
            let expected = hash_from_proto("expected_hash", expected)?;
            let actual = ObjectHash::digest(ObjectKind::Blob, &bytes);
            if expected != actual {
                return Err(Error::invalid(format!(
                    "content hash mismatch: expected {expected}, got {actual}"
                )));
            }
        }
        let content = store_content(opened.repo.repo.objects().as_ref(), bytes).await?;
        Ok(pb::PutBlobResponse {
            content: Some(pb::ContentRef::from(content)),
        })
    }

    pub(super) async fn has_objects_impl(
        &self,
        request: tonic::Request<pb::HasObjectsRequest>,
    ) -> Result<pb::HasObjectsResponse> {
        let req = request.get_ref();
        let opened = open(&self.state, &request, &req.repo).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let mut items = Vec::with_capacity(req.objects.len());
        for o in &req.objects {
            let kind = pb::ObjectKind::try_from(o.kind)
                .ok()
                .and_then(|k| ObjectKind::from_u8(k as u8))
                .ok_or_else(|| Error::invalid(format!("unknown object kind {}", o.kind)))?;
            items.push((kind, hash_from_proto("objects.hash", &o.hash)?));
        }
        let present = opened.repo.repo.objects().has_many(&items).await?;
        Ok(pb::HasObjectsResponse { present })
    }

    pub(super) async fn diff_impl(
        &self,
        request: tonic::Request<pb::DiffRequest>,
    ) -> Result<pb::DiffResponse> {
        let req = request.get_ref();
        let opened = open(&self.state, &request, &req.repo).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let filter = opened.permit().read_filter().await?;
        let ctx = &opened.repo;
        let base = resolve_root(ctx, req.base.as_ref()).await?;
        let target = resolve_root(ctx, req.target.as_ref()).await?;
        let target_cv = match target.version {
            Some(v) => Some(ctx.derive.change(&v).await?),
            None => None,
        };
        let options =
            DiffOptions::new(&req.paths, req.format, req.context_lines, req.limit, filter)?;
        let (files, truncated) = file_diffs(
            ctx,
            Some(base.root),
            Some(target.root),
            target_cv.as_ref(),
            &options,
        )
        .await?;
        Ok(pb::DiffResponse { files, truncated })
    }

    pub(super) async fn merge_base_impl(
        &self,
        request: tonic::Request<pb::MergeBaseRequest>,
    ) -> Result<pb::MergeBaseResponse> {
        let req = request.get_ref();
        let opened = open(&self.state, &request, &req.repo).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let a = hash("a", &req.a)?;
        let b = hash("b", &req.b)?;
        let base = opened.repo.graph.merge_base(&a, &b).await?;
        Ok(pb::MergeBaseResponse {
            bases: base.map(|h| vec![hash_to_proto(&h)]).unwrap_or_default(),
        })
    }

    pub(super) async fn is_ancestor_impl(
        &self,
        request: tonic::Request<pb::IsAncestorRequest>,
    ) -> Result<pb::IsAncestorResponse> {
        let req = request.get_ref();
        let opened = open(&self.state, &request, &req.repo).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
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
