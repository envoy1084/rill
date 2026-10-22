//! `ContentService` implementation backed by [`MockRepo`].

use std::collections::BTreeMap;

use bytes::Bytes;
use connectrpc::{
    ConnectError, InboundStream, RequestContext, Response, ServiceRequest, ServiceStream,
};
use rill_proto::convert::{hash_from_proto, hash_to_proto};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::ContentService;
use rill_types::{ObjectHash, ObjectKind};

use super::fixtures::fake_hash;
use super::mock_repo::MockRepo;
use super::rpc::{next_message, unimplemented_rpcs, Request, RpcResult};

// The mocks return concrete types.
#[allow(refining_impl_trait)]
impl ContentService for MockRepo {
    async fn list_directory(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::ListDirectoryRequest>,
    ) -> RpcResult<pb::ListDirectoryResponse> {
        let req = Request::new(ctx, req).into_inner();
        let tree = self.tree(&req.revision)?;
        let prefix = if req.path.is_empty() {
            String::new()
        } else {
            format!("{}/", req.path)
        };
        let mut entries: BTreeMap<String, pb::TreeEntry> = BTreeMap::new();
        for (path, file) in &tree {
            let Some(rest) = path.strip_prefix(&prefix) else {
                continue;
            };
            match rest.split_once('/') {
                Some((dir, _)) => {
                    entries.entry(dir.to_string()).or_insert(pb::TreeEntry {
                        kind: Some(pb::tree_entry::Kind::Dir(pb::tree_entry::Dir {
                            hash: hash_to_proto(&fake_hash(&format!("{prefix}{dir}"))),
                            summary: pb::DirSummary::default().into(),
                        })),
                    });
                }
                None => {
                    entries.insert(rest.to_string(), self.entry(file));
                }
            }
        }
        Ok(Response::new(pb::ListDirectoryResponse {
            tree_hash: hash_to_proto(&fake_hash(&req.path)),
            entries: entries
                .into_iter()
                .map(|(name, entry)| pb::NamedTreeEntry {
                    name,
                    entry: entry.into(),
                })
                .collect(),
            next_page_token: String::new(),
            read_grant: String::new(),
        }))
    }

    async fn stat(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::StatRequest>,
    ) -> RpcResult<pb::StatResponse> {
        let req = Request::new(ctx, req).into_inner();
        let tree = self.tree(&req.revision)?;
        Ok(Response::new(pb::StatResponse {
            entry: (tree.get(&req.path).map(|f| self.entry(f))).into(),
            read_grant: String::new(),
        }))
    }

    async fn read_file(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::ReadFileRequest>,
    ) -> RpcResult<pb::ReadFileResponse> {
        let req = Request::new(ctx, req).into_inner();
        let tree = self.tree(&req.revision)?;
        let file = tree
            .get(&req.path)
            .ok_or_else(|| ConnectError::not_found(format!("{} not found", req.path)))?;
        let entry = self.entry(file);
        // Large files come back by reference, as the server does.
        let truncated = file.bytes.len() > 4096;
        Ok(Response::new(pb::ReadFileResponse {
            entry: entry.into(),
            data: if truncated {
                Bytes::new()
            } else {
                file.bytes.clone()
            },
            truncated,
            read_grant: String::new(),
        }))
    }

    async fn read_blob(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::ReadBlobRequest>,
    ) -> RpcResult<ServiceStream<pb::ReadBlobResponse>> {
        let req = Request::new(ctx, req).into_inner();
        let hash = match req.content.into_option().and_then(|c| c.r#ref) {
            Some(pb::content_ref::Ref::Blob(b)) | Some(pb::content_ref::Ref::Chunked(b)) => {
                hash_from_proto("hash", &b.hash)?
            }
            Some(pb::content_ref::Ref::Inline(bytes)) => {
                let chunk = pb::ReadBlobResponse { data: bytes };
                return Ok(Response::new(Box::pin(tokio_stream::once(Ok(chunk)))));
            }
            None => return Err(ConnectError::invalid_argument("missing content")),
        };
        let bytes = self
            .blobs
            .lock()
            .unwrap()
            .get(&hash)
            .cloned()
            .ok_or_else(|| ConnectError::not_found("blob"))?;
        let chunk_size = self.chunk_size;
        let chunks: Vec<_> = (0..bytes.len())
            .step_by(chunk_size)
            .map(|start| {
                Ok(pb::ReadBlobResponse {
                    data: bytes.slice(start..(start + chunk_size).min(bytes.len())),
                })
            })
            .collect();
        Ok(Response::new(Box::pin(tokio_stream::iter(chunks))))
    }

    async fn write_blob(
        &self,
        ctx: RequestContext,
        requests: InboundStream<pb::WriteBlobRequest>,
    ) -> RpcResult<pb::WriteBlobResponse> {
        let mut stream = Request::with_message(ctx, requests).into_inner();
        let mut data = Vec::new();
        let mut messages = 0usize;
        let mut saw_header = false;
        while let Some(msg) = next_message(&mut stream).await? {
            messages += 1;
            match msg.msg {
                Some(pb::write_blob_request::Msg::Header(h)) => {
                    saw_header = true;
                    if h.repo.is_empty() || h.request_id.is_empty() {
                        return Err(ConnectError::invalid_argument("bad header"));
                    }
                }
                Some(pb::write_blob_request::Msg::Data(d)) => data.extend_from_slice(&d),
                None => return Err(ConnectError::invalid_argument("empty message")),
            }
        }
        if !saw_header {
            return Err(ConnectError::invalid_argument("no header"));
        }
        self.put_requests.lock().unwrap().push(messages);
        let bytes = Bytes::from(data);
        let hash = ObjectHash::digest(ObjectKind::Blob, &bytes);
        let size = bytes.len() as u64;
        self.blobs.lock().unwrap().insert(hash, bytes);
        Ok(Response::new(pb::WriteBlobResponse {
            content: (pb::ContentRef {
                r#ref: Some(pb::content_ref::Ref::Blob(pb::BlobRef {
                    hash: hash_to_proto(&hash),
                    size,
                })),
            })
            .into(),
        }))
    }

    async fn list_history(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::ListHistoryRequest>,
    ) -> RpcResult<pb::ListHistoryResponse> {
        let req = Request::new(ctx, req).into_inner();
        let versions = self
            .log
            .lock()
            .unwrap()
            .iter()
            .take(req.page_size.max(0) as usize)
            .cloned()
            .collect();
        Ok(Response::new(pb::ListHistoryResponse {
            versions,
            next_page_token: String::new(),
        }))
    }

    unimplemented_rpcs! {
        batch_read_files(pb::BatchReadFilesRequest) -> pb::BatchReadFilesResponse;
        get_tree(pb::GetTreeRequest) -> pb::GetTreeResponse;
        batch_check_objects(pb::BatchCheckObjectsRequest) -> pb::BatchCheckObjectsResponse;
        diff(pb::DiffRequest) -> pb::DiffResponse;
        blame(pb::BlameRequest) -> pb::BlameResponse;
        list_path_history(pb::ListPathHistoryRequest) -> pb::ListPathHistoryResponse;
        grep(pb::GrepRequest) -> pb::GrepResponse;
        find_merge_base(pb::FindMergeBaseRequest) -> pb::FindMergeBaseResponse;
        is_ancestor(pb::IsAncestorRequest) -> pb::IsAncestorResponse;
    }
}
