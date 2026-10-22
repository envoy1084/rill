//! `ContentService` implementation backed by [`MockRepo`].

use std::collections::BTreeMap;

use bytes::Bytes;
use connectrpc::{
    ConnectError, InboundStream, RequestContext, Response, ServiceRequest, ServiceStream,
};
use rill_proto::convert::{hash_from_proto, hash_to_proto};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::ContentService;

use super::fixtures::fake_hash;
use super::mock_repo::MockRepo;
use super::rpc::{unimplemented_rpcs, Request, RpcResult};

// The mocks return concrete types.
#[allow(refining_impl_trait)]
impl ContentService for MockRepo {
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
        for (path, bytes) in &tree {
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
                    entries.insert(rest.to_string(), self.entry(bytes));
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
            entry: (tree.get(&req.path).map(|b| self.entry(b))).into(),
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
        let bytes = tree
            .get(&req.path)
            .ok_or_else(|| ConnectError::not_found(format!("{} not found", req.path)))?;
        Ok(Response::new(pb::ReadFileResponse {
            entry: self.entry(bytes).into(),
            data: bytes.clone(),
            truncated: false,
            read_grant: String::new(),
        }))
    }

    async fn read_blob(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::ReadBlobRequest>,
    ) -> RpcResult<ServiceStream<pb::ReadBlobResponse>> {
        let req = Request::new(ctx, req).into_inner();
        let data = match req.content.into_option().and_then(|c| c.r#ref) {
            Some(pb::content_ref::Ref::Blob(b)) | Some(pb::content_ref::Ref::Chunked(b)) => {
                let hash = hash_from_proto("hash", &b.hash)?;
                self.blobs
                    .lock()
                    .unwrap()
                    .get(&hash)
                    .cloned()
                    .ok_or_else(|| ConnectError::not_found("blob"))?
            }
            Some(pb::content_ref::Ref::Inline(bytes)) => bytes,
            None => return Err(ConnectError::invalid_argument("missing content")),
        };
        Ok(Response::new(Box::pin(tokio_stream::once(Ok(
            pb::ReadBlobResponse { data },
        )))))
    }

    async fn write_blob(
        &self,
        _ctx: RequestContext,
        _requests: InboundStream<pb::WriteBlobRequest>,
    ) -> RpcResult<pb::WriteBlobResponse> {
        Err(ConnectError::unimplemented("write_blob"))
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
