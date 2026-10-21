//! Blob transfer: streamed reads, chunked uploads and existence checks.

use bytes::{Bytes, BytesMut};
use rill_proto::convert::{hash_to_proto, id_to_proto, required};
use rill_proto::rill::v1 as pb;
use rill_types::{ContentRef, FileKind, ObjectHash, ObjectKind, OperationId, INLINE_THRESHOLD};
use tokio_stream::StreamExt;

use super::RepoClient;
use crate::error::Result;

/// Size of each data message in a `PutBlob` stream.
pub const PUT_BLOB_CHUNK_SIZE: usize = 256 * 1024;

impl RepoClient {
    /// The bytes behind a content reference. Inline content is returned
    /// without a round-trip.
    pub async fn get_blob(&self, content: &ContentRef) -> Result<Bytes> {
        if let ContentRef::Inline { bytes } = content {
            return Ok(bytes.clone());
        }
        self.get_blob_range(content, None, None).await
    }

    /// A byte range of a blob; `None` bounds mean the start or the end.
    pub async fn get_blob_range(
        &self,
        content: &ContentRef,
        offset: Option<u64>,
        length: Option<u64>,
    ) -> Result<Bytes> {
        if let ContentRef::Inline { bytes } = content {
            let start = offset.unwrap_or(0).min(bytes.len() as u64) as usize;
            let end = length
                .map(|l| (start as u64 + l).min(bytes.len() as u64) as usize)
                .unwrap_or(bytes.len());
            return Ok(bytes.slice(start..end));
        }
        let mut stream = crate::client::retry_idempotent(|| {
            let mut svc = self.svc();
            let request = pb::GetBlobRequest {
                repo: self.repo.clone(),
                content: Some(pb::ContentRef::from(content)),
                offset,
                length,
            };
            async move { svc.get_blob(request).await }
        })
        .await?
        .into_inner();
        let capacity = length.unwrap_or_else(|| content.size()) as usize;
        let mut out = BytesMut::with_capacity(capacity);
        while let Some(chunk) = stream.next().await {
            out.extend_from_slice(&chunk?.data);
        }
        Ok(out.freeze())
    }

    /// Upload bytes as a blob, streamed in [`PUT_BLOB_CHUNK_SIZE`] pieces.
    /// The server decides whether to chunk the blob further.
    pub async fn put_blob(&self, bytes: Bytes) -> Result<ContentRef> {
        self.put_blob_with_op(OperationId::new(), bytes).await
    }

    /// [`RepoClient::put_blob`] with a caller-chosen idempotency key.
    pub async fn put_blob_with_op(&self, op: OperationId, bytes: Bytes) -> Result<ContentRef> {
        let header = pb::PutBlobRequest {
            msg: Some(pb::put_blob_request::Msg::Header(pb::PutBlobHeader {
                operation_id: id_to_proto(&op),
                repo: self.repo.clone(),
                size: Some(bytes.len() as u64),
                expected_hash: None,
            })),
        };
        let total = bytes.len();
        let resp = crate::client::retry_idempotent(|| {
            let bytes = bytes.clone();
            let chunks = (0..total).step_by(PUT_BLOB_CHUNK_SIZE).map(move |start| {
                let end = (start + PUT_BLOB_CHUNK_SIZE).min(total);
                pb::PutBlobRequest {
                    msg: Some(pb::put_blob_request::Msg::Data(bytes.slice(start..end))),
                }
            });
            let stream = tokio_stream::iter(std::iter::once(header.clone()).chain(chunks));
            let mut svc = self.svc();
            async move { svc.put_blob(stream).await }
        })
        .await?
        .into_inner();
        Ok(required("put_blob.content", resp.content)?.try_into()?)
    }

    /// Turn file bytes into a content reference: inline when small enough,
    /// otherwise uploaded as a blob.
    pub async fn store_content(&self, bytes: Bytes) -> Result<ContentRef> {
        if bytes.len() <= INLINE_THRESHOLD {
            Ok(ContentRef::Inline { bytes })
        } else {
            self.put_blob(bytes).await
        }
    }

    /// Which of the given objects the server already has, in order.
    pub async fn has_objects(&self, objects: &[(ObjectKind, ObjectHash)]) -> Result<Vec<bool>> {
        let resp = crate::client::retry_idempotent(|| {
            let mut svc = self.svc();
            let request = pb::HasObjectsRequest {
                repo: self.repo.clone(),
                objects: objects
                    .iter()
                    .map(|(kind, hash)| pb::ObjectRef {
                        kind: object_kind_to_proto(*kind) as i32,
                        hash: hash_to_proto(hash),
                    })
                    .collect(),
            };
            async move { svc.has_objects(request).await }
        })
        .await?
        .into_inner();
        Ok(resp.present)
    }
}

fn object_kind_to_proto(kind: ObjectKind) -> pb::ObjectKind {
    match kind {
        ObjectKind::Blob => pb::ObjectKind::Blob,
        ObjectKind::Chunk => pb::ObjectKind::Chunk,
        ObjectKind::BlobManifest => pb::ObjectKind::BlobManifest,
        ObjectKind::Tree => pb::ObjectKind::Tree,
        ObjectKind::ChangeVersion => pb::ObjectKind::ChangeVersion,
        ObjectKind::Derived => pb::ObjectKind::Derived,
    }
}

/// Helper shared by edits and resolutions: content plus kind, uploaded.
pub(crate) async fn stored_set(
    repo: &RepoClient,
    bytes: Bytes,
    kind: FileKind,
) -> Result<pb::SetContent> {
    let content = repo.store_content(bytes).await?;
    Ok(crate::types::set_content_proto(content, kind))
}
