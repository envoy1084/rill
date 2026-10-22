//! `RepoService` implementation backed by [`MockRepo`].

use std::collections::BTreeMap;

use futures::stream::BoxStream;
use rill_proto::convert::{hash_from_proto, hash_to_proto, id_to_proto};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::repo_service_server::RepoService;
use tonic::{Request, Response, Status, Streaming};

use super::fixtures::fake_hash;
use super::mock_repo::MockRepo;
use super::rpc::{unimplemented_rpcs, BoxFut, RpcResult};

#[tonic::async_trait]
impl RepoService for MockRepo {
    type GetBlobStream = BoxStream<'static, Result<pb::BlobChunk, Status>>;
    type WatchRefsStream = BoxStream<'static, Result<pb::RefEvent, Status>>;
    type WatchTrunkStream = BoxStream<'static, Result<pb::LandEvent, Status>>;
    type WatchLogStream = BoxStream<'static, Result<pb::LogEntry, Status>>;
    type WatchEventsStream = BoxStream<'static, Result<pb::EventEnvelope, Status>>;

    async fn get_repo(&self, req: Request<pb::GetRepoRequest>) -> RpcResult<pb::GetRepoResponse> {
        let name = req.into_inner().repo;
        Ok(Response::new(pb::GetRepoResponse {
            repo: Some(pb::Repo {
                id: id_to_proto(&rill_types::RepoId::derived("test", &name)),
                name,
                description: Some("mock".into()),
                created_at: 1,
                trunk_head_seq: 3,
                windows_safe: false,
                org: "default".into(),
            }),
        }))
    }

    async fn log(&self, req: Request<pb::LogRequest>) -> RpcResult<pb::LogResponse> {
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

    async fn list_dir(&self, req: Request<pb::ListDirRequest>) -> RpcResult<pb::ListDirResponse> {
        let req = req.into_inner();
        let tree = self.tree(req.root.as_ref())?;
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
                            summary: Some(pb::DirSummary::default()),
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
                    entry: Some(entry),
                })
                .collect(),
            read_grant: String::new(),
        }))
    }

    async fn resolve_path(
        &self,
        req: Request<pb::ResolvePathRequest>,
    ) -> RpcResult<pb::ResolvePathResponse> {
        let req = req.into_inner();
        let tree = self.tree(req.root.as_ref())?;
        Ok(Response::new(pb::ResolvePathResponse {
            entry: tree.get(&req.path).map(|b| self.entry(b)),
            read_grant: String::new(),
        }))
    }

    async fn get_blob(&self, req: Request<pb::GetBlobRequest>) -> RpcResult<Self::GetBlobStream> {
        let req = req.into_inner();
        let hash = match req.content.and_then(|c| c.r#ref) {
            Some(pb::content_ref::Ref::Blob(b)) | Some(pb::content_ref::Ref::Chunked(b)) => {
                hash_from_proto("hash", &b.hash)?
            }
            Some(pb::content_ref::Ref::Inline(bytes)) => {
                let chunk = pb::BlobChunk { data: bytes };
                return Ok(Response::new(Box::pin(tokio_stream::once(Ok(chunk)))));
            }
            None => return Err(Status::invalid_argument("missing content")),
        };
        let bytes = self
            .blobs
            .lock()
            .unwrap()
            .get(&hash)
            .cloned()
            .ok_or_else(|| Status::not_found("blob"))?;
        Ok(Response::new(Box::pin(tokio_stream::once(Ok(
            pb::BlobChunk { data: bytes },
        )))))
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
        put_blob(Streaming<pb::PutBlobRequest>) -> pb::PutBlobResponse;
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
        watch_trunk(pb::WatchTrunkRequest) -> Self::WatchTrunkStream;
        watch_refs(pb::WatchRefsRequest) -> Self::WatchRefsStream;
        watch_log(pb::WatchLogRequest) -> Self::WatchLogStream;
        watch_events(pb::WatchEventsRequest) -> Self::WatchEventsStream;
        get_derived(pb::GetDerivedRequest) -> pb::GetDerivedResponse;
        wait_derived(pb::WaitDerivedRequest) -> pb::WaitDerivedResponse;
    }
}
