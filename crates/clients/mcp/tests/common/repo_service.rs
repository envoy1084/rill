//! `RepoService` implementation backed by [`MockRepo`].

use std::collections::BTreeMap;

use connectrpc::{
    ConnectError, InboundStream, RequestContext, Response, ServiceRequest, ServiceStream,
};
use futures::stream::BoxStream;
use rill_proto::convert::{hash_from_proto, hash_to_proto, id_to_proto};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::RepoService;

use super::fixtures::fake_hash;
use super::mock_repo::MockRepo;
use super::rpc::{unimplemented_rpcs, Request, RpcResult};

// The mocks return concrete types.
#[allow(refining_impl_trait)]
impl RepoService for MockRepo {
    async fn get_repo(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::GetRepoRequest>,
    ) -> RpcResult<pb::GetRepoResponse> {
        let req = Request::new(ctx, req);
        let name = req.into_inner().repo;
        Ok(Response::new(pb::GetRepoResponse {
            repo: (pb::Repo {
                id: id_to_proto(&rill_types::RepoId::derived("test", &name)),
                name,
                description: Some("mock".into()),
                created_at: 1,
                trunk_head_seq: 3,
                windows_safe: false,
                org: "default".into(),
            })
            .into(),
        }))
    }

    async fn log(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::LogRequest>,
    ) -> RpcResult<pb::LogResponse> {
        let req = Request::new(ctx, req);
        let req = req.into_inner();
        let entries: Vec<_> = self
            .log
            .lock()
            .unwrap()
            .iter()
            .take(req.limit as usize)
            .cloned()
            .collect();
        Ok(Response::new(pb::LogResponse {
            entries,
            next_start: None,
        }))
    }

    async fn list_dir(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::ListDirRequest>,
    ) -> RpcResult<pb::ListDirResponse> {
        let req = Request::new(ctx, req);
        let req = req.into_inner();
        let tree = self.tree(req.root.as_option())?;
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
        Ok(Response::new(pb::ListDirResponse {
            tree_hash: hash_to_proto(&fake_hash(&req.path)),
            entries: entries
                .into_iter()
                .map(|(name, entry)| pb::NamedTreeEntry {
                    name,
                    entry: entry.into(),
                })
                .collect(),
            read_grant: String::new(),
        }))
    }

    async fn resolve_path(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::ResolvePathRequest>,
    ) -> RpcResult<pb::ResolvePathResponse> {
        let req = Request::new(ctx, req);
        let req = req.into_inner();
        let tree = self.tree(req.root.as_option())?;
        Ok(Response::new(pb::ResolvePathResponse {
            entry: (tree.get(&req.path).map(|b| self.entry(b))).into(),
            read_grant: String::new(),
        }))
    }

    async fn get_blob(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::GetBlobRequest>,
    ) -> RpcResult<ServiceStream<pb::BlobChunk>> {
        let req = Request::new(ctx, req);
        let req = req.into_inner();
        let hash = match req.content.into_option().and_then(|c| c.r#ref) {
            Some(pb::content_ref::Ref::Blob(b)) | Some(pb::content_ref::Ref::Chunked(b)) => {
                hash_from_proto("hash", &b.hash)?
            }
            Some(pb::content_ref::Ref::Inline(bytes)) => {
                let chunk = pb::BlobChunk { data: bytes };
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
        Ok(Response::new(Box::pin(tokio_stream::once(Ok(
            pb::BlobChunk { data: bytes },
        )))))
    }

    async fn put_blob(
        &self,
        _ctx: RequestContext,
        _requests: InboundStream<pb::PutBlobRequest>,
    ) -> RpcResult<pb::PutBlobResponse> {
        Err(ConnectError::unimplemented("put_blob"))
    }

    unimplemented_rpcs! {
        list_repos(pb::ListReposRequest) -> pb::ListReposResponse;
        create_repo(pb::CreateRepoRequest) -> pb::CreateRepoResponse;
        get_change_version(pb::GetChangeVersionRequest) -> pb::GetChangeVersionResponse;
        get_change_info(pb::GetChangeInfoRequest) -> pb::GetChangeInfoResponse;
        get_change_versions(pb::GetChangeVersionsRequest) -> pb::GetChangeVersionsResponse;
        path_history(pb::PathHistoryRequest) -> pb::PathHistoryResponse;
        grep(pb::GrepRequest) -> pb::GrepResponse;
        blame(pb::BlameRequest) -> pb::BlameResponse;
        get_tree(pb::GetTreeRequest) -> pb::GetTreeResponse;
        has_objects(pb::HasObjectsRequest) -> pb::HasObjectsResponse;
        diff(pb::DiffRequest) -> pb::DiffResponse;
        merge_base(pb::MergeBaseRequest) -> pb::MergeBaseResponse;
        is_ancestor(pb::IsAncestorRequest) -> pb::IsAncestorResponse;
        create_change_version(pb::CreateChangeVersionRequest) -> pb::CreateChangeVersionResponse;
        apply_edits(pb::ApplyEditsRequest) -> pb::ApplyEditsResponse;
        rebase(pb::RebaseRequest) -> pb::RebaseResponse;
        merge(pb::MergeRequest) -> pb::MergeResponse;
        resolve(pb::ResolveRequest) -> pb::ResolveResponse;
        resolve_ref(pb::ResolveRefRequest) -> pb::ResolveRefResponse;
        list_refs(pb::ListRefsRequest) -> pb::ListRefsResponse;
        move_ref(pb::MoveRefRequest) -> pb::MoveRefResponse;
        cut_release(pb::CutReleaseRequest) -> pb::CutReleaseResponse;
        trunk_head(pb::TrunkHeadRequest) -> pb::TrunkHeadResponse;
        trunk_range(pb::TrunkRangeRequest) -> pb::TrunkRangeResponse;
        watch_trunk(pb::WatchTrunkRequest) -> ServiceStream<pb::LandEvent>;
        watch_refs(pb::WatchRefsRequest) -> ServiceStream<pb::RefEvent>;
        watch_log(pb::WatchLogRequest) -> ServiceStream<pb::LogEntry>;
        watch_events(pb::WatchEventsRequest) -> ServiceStream<pb::EventEnvelope>;
        get_derived(pb::GetDerivedRequest) -> pb::GetDerivedResponse;
        wait_derived(pb::WaitDerivedRequest) -> pb::WaitDerivedResponse;
    }
}
