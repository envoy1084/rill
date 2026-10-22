//! Blobs: streaming content out, uploading it, and checking what exists.

use std::pin::Pin;

use bytes::Bytes;
use futures::{Stream, StreamExt};
use rill_core::{content_parts, read_content, store_content_stream};
use rill_proto::convert::hash_from_proto;
use rill_proto::rill::v1 as pb;
use rill_store::ObjectStore;
use rill_types::{ContentRef, ObjectKind};

use super::ContentSvc;
use crate::grants;
use crate::services::call::next_message;
use crate::services::common::open_repo;
use crate::services::Call;
use connectrpc::{ConnectError, InboundStream};
use rill_runtime::error::{Error, Result};

/// Size of each `ReadBlob` chunk.
pub const BLOB_CHUNK: usize = 256 * 1024;

/// Server stream of blob chunks.
pub type BlobStream =
    Pin<Box<dyn Stream<Item = std::result::Result<pb::ReadBlobResponse, ConnectError>> + Send>>;

impl ContentSvc {
    pub(super) async fn read_blob_impl(
        &self,
        request: Call<pb::ReadBlobRequest>,
    ) -> Result<BlobStream> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.repo).await?;
        let content = ContentRef::try_from(
            req.content
                .clone()
                .into_option()
                .ok_or(rill_proto::ConvertError::missing("content"))?,
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
            let pieces: Vec<std::result::Result<pb::ReadBlobResponse, ConnectError>> = match part {
                Ok(part) => (0..part.len())
                    .step_by(BLOB_CHUNK)
                    .map(|start| {
                        Ok(pb::ReadBlobResponse {
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

    pub(super) async fn write_blob_impl(
        &self,
        request: Call<InboundStream<pb::WriteBlobRequest>>,
    ) -> Result<pb::WriteBlobResponse> {
        let ctx = crate::auth::caller(&request)?;
        let mut stream = request.into_inner();
        // The header comes first, so permission is checked before any data
        // is accepted.
        let header = match next_message(&mut stream).await? {
            Some(pb::WriteBlobRequest {
                msg: Some(pb::write_blob_request::Msg::Header(h)),
            }) => h,
            _ => return Err(Error::invalid("WriteBlob must start with a header")),
        };
        let repo = self
            .state
            .repos
            .resolve(&crate::services::names::repo_spec(&header.repo)?)
            .await?;
        let opened = crate::services::common::opened(&self.state, ctx, repo);
        // The commit naming this content checks its path.
        opened.permit().write_any().await?;

        let parts = futures::stream::unfold(stream, |mut stream| async move {
            match next_message(&mut stream).await {
                Ok(Some(pb::WriteBlobRequest {
                    msg: Some(pb::write_blob_request::Msg::Data(data)),
                })) => Some((Ok(data), stream)),
                Ok(Some(pb::WriteBlobRequest {
                    msg: Some(pb::write_blob_request::Msg::Header(_)),
                })) => Some((
                    Err(rill_core::Error::Upload(
                        "duplicate WriteBlob header".into(),
                    )),
                    stream,
                )),
                Ok(Some(pb::WriteBlobRequest { msg: None })) => Some((Ok(Bytes::new()), stream)),
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
                    "WriteBlob header announced {size} bytes but {} arrived",
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
        Ok(pb::WriteBlobResponse {
            content: pb::ContentRef::from(stored.content).into(),
        })
    }

    pub(super) async fn batch_check_objects_impl(
        &self,
        request: Call<pb::BatchCheckObjectsRequest>,
    ) -> Result<pb::BatchCheckObjectsResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.repo).await?;
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
        Ok(pb::BatchCheckObjectsResponse { present })
    }
}

/// Verify that the stored object a content ref points at exists.
pub async fn check_content_exists(objects: &dyn ObjectStore, content: &ContentRef) -> Result<()> {
    if let Some((kind, hash)) = content.stored_hash() {
        if !objects.has(kind, &hash).await? {
            return Err(Error::MissingObjects(format!(
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
