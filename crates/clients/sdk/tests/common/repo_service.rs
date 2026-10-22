//! `RepoService` implementation backed by [`MockRepo`].

use std::collections::BTreeMap;

use bytes::Bytes;
use connectrpc::{
    ConnectError, InboundStream, RequestContext, Response, ServiceRequest, ServiceStream,
};
use futures::stream::BoxStream;
use rill_proto::convert::{hash_from_proto, hash_to_proto, id_to_proto};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::RepoService;
use rill_types::{ChangeId, ObjectHash, ObjectKind, Principal};

use super::fixtures::{fake_hash, sample_info};
use super::mock_repo::MockRepo;
use super::rpc::{hex, next_message, unimplemented_rpcs, Request, RpcResult};

// The mocks return concrete types.
#[allow(refining_impl_trait)]
impl RepoService for MockRepo {
    async fn get_repo(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::GetRepoRequest>,
    ) -> RpcResult<pb::GetRepoResponse> {
        let req = Request::new(ctx, req);
        self.record_auth(&req);
        let name = req.into_inner().repo;
        if name == "missing" {
            return Err(ConnectError::not_found("no such repo"));
        }
        Ok(Response::new(pb::GetRepoResponse {
            repo: (pb::Repo {
                id: id_to_proto(&rill_types::RepoId::derived("test", &name)),
                name,
                description: Some("mock".into()),
                created_at: 1,
                trunk_head_seq: self.trunk.lock().unwrap().0,
                windows_safe: false,
                org: "default".into(),
            })
            .into(),
        }))
    }

    async fn get_change_versions(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::GetChangeVersionsRequest>,
    ) -> RpcResult<pb::GetChangeVersionsResponse> {
        let req = Request::new(ctx, req);
        let id = hex(&req.into_inner().change_id);
        Ok(Response::new(pb::GetChangeVersionsResponse {
            versions: vec![sample_info(&id, None)],
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
            entry: (tree.get(&req.path).map(|f| self.entry(f))).into(),
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
        let chunk_size = self.chunk_size;
        let chunks: Vec<_> = (0..bytes.len())
            .step_by(chunk_size)
            .map(|start| {
                Ok(pb::BlobChunk {
                    data: bytes.slice(start..(start + chunk_size).min(bytes.len())),
                })
            })
            .collect();
        Ok(Response::new(Box::pin(tokio_stream::iter(chunks))))
    }

    async fn put_blob(
        &self,
        ctx: RequestContext,
        requests: InboundStream<pb::PutBlobRequest>,
    ) -> RpcResult<pb::PutBlobResponse> {
        let req = Request::with_message(ctx, requests);
        let mut stream = req.into_inner();
        let mut data = Vec::new();
        let mut messages = 0usize;
        let mut saw_header = false;
        while let Some(msg) = next_message(&mut stream).await? {
            messages += 1;
            match msg.msg {
                Some(pb::put_blob_request::Msg::Header(h)) => {
                    saw_header = true;
                    if h.repo.is_empty() || h.operation_id.len() != 16 {
                        return Err(ConnectError::invalid_argument("bad header"));
                    }
                }
                Some(pb::put_blob_request::Msg::Data(d)) => data.extend_from_slice(&d),
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
        Ok(Response::new(pb::PutBlobResponse {
            content: (pb::ContentRef {
                r#ref: Some(pb::content_ref::Ref::Blob(pb::BlobRef {
                    hash: hash_to_proto(&hash),
                    size,
                })),
            })
            .into(),
        }))
    }

    async fn apply_edits(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::ApplyEditsRequest>,
    ) -> RpcResult<pb::ApplyEditsResponse> {
        let req = Request::new(ctx, req);
        let req = req.into_inner();
        self.applied.lock().unwrap().push(req.clone());
        let failing = self.unavailable.load(std::sync::atomic::Ordering::SeqCst);
        if failing > 0 {
            self.unavailable
                .store(failing - 1, std::sync::atomic::Ordering::SeqCst);
            return Err(ConnectError::unavailable("server going away"));
        }
        let mut file_changes = BTreeMap::new();
        for edit in &req.edits {
            let change = match &edit.op {
                Some(pb::edit::Op::SetContent(set)) => {
                    pb::file_change::Change::Set(pb::file_change::Set {
                        content: set.content.clone(),
                        kind: set.kind,
                        copy_from: None.into(),
                    })
                }
                Some(pb::edit::Op::Delete(_)) => {
                    pb::file_change::Change::Delete(pb::file_change::Delete {})
                }
                Some(pb::edit::Op::Patch(p)) => {
                    pb::file_change::Change::Set(pb::file_change::Set {
                        content: (pb::ContentRef {
                            r#ref: Some(pb::content_ref::Ref::Inline(Bytes::from(p.clone()))),
                        })
                        .into(),
                        kind: pb::FileKind::Regular.into(),
                        copy_from: None.into(),
                    })
                }
                None => return Err(ConnectError::invalid_argument("edit without op")),
            };
            file_changes.insert(
                edit.path.clone(),
                pb::FileChange {
                    change: Some(change),
                },
            );
        }
        let change_id = req
            .change_id
            .clone()
            .unwrap_or_else(|| id_to_proto(&ChangeId::new()));
        let version = pb::ChangeVersion {
            change_id,
            parents: vec![hash_to_proto(&self.trunk.lock().unwrap().1)],
            predecessor: None,
            subtree_changes: BTreeMap::new(),
            file_changes,
            author: (pb::Principal::from(Principal::human("alice"))).into(),
            provenance: req.provenance.clone(),
            intent: req.intent.clone(),
            message: req.message.clone(),
            created_at: 42,
            extra: BTreeMap::new(),
        };
        let hash = fake_hash(&format!("apply:{}:{}", req.message, req.edits.len()));
        Ok(Response::new(pb::ApplyEditsResponse {
            hash: hash_to_proto(&hash),
            version: version.into(),
        }))
    }

    async fn rebase(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::RebaseRequest>,
    ) -> RpcResult<pb::RebaseResponse> {
        let req = Request::new(ctx, req);
        let req = req.into_inner();
        let from = hash_from_proto("hash", &req.hash)?;
        let onto = hash_from_proto("new_parent", &req.new_parent)?;
        self.rebases.lock().unwrap().push((from, onto));
        let new_hash = fake_hash(&format!("rebase:{from}:{onto}"));
        Ok(Response::new(pb::RebaseResponse {
            hash: hash_to_proto(&new_hash),
            version: (pb::ChangeVersion {
                change_id: id_to_proto(&ChangeId::derived("rebase", &from.to_hex())),
                parents: vec![req.new_parent],
                predecessor: Some(req.hash),
                subtree_changes: BTreeMap::new(),
                file_changes: BTreeMap::new(),
                author: (pb::Principal::from(Principal::human("alice"))).into(),
                provenance: None.into(),
                intent: None,
                message: "rebased".into(),
                created_at: 1,
                extra: BTreeMap::new(),
            })
            .into(),
        }))
    }

    async fn resolve_ref(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::ResolveRefRequest>,
    ) -> RpcResult<pb::ResolveRefResponse> {
        let req = Request::new(ctx, req);
        let name = req.into_inner().name;
        if name != "trunk" {
            return Err(ConnectError::not_found(format!("ref {name}")));
        }
        let (seq, hash) = *self.trunk.lock().unwrap();
        Ok(Response::new(pb::ResolveRefResponse {
            hash: hash_to_proto(&hash),
            trunk_seq: Some(seq),
        }))
    }

    async fn trunk_head(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::TrunkHeadRequest>,
    ) -> RpcResult<pb::TrunkHeadResponse> {
        let req = Request::new(ctx, req);
        self.record_auth(&req);
        let (seq, hash) = *self.trunk.lock().unwrap();
        Ok(Response::new(pb::TrunkHeadResponse {
            seq,
            hash: hash_to_proto(&hash),
        }))
    }

    async fn watch_trunk(
        &self,
        ctx: RequestContext,
        _req: ServiceRequest<'_, pb::WatchTrunkRequest>,
    ) -> RpcResult<ServiceStream<pb::LandEvent>> {
        let _req = Request::new(ctx, _req);
        let events = (1..=3u64).map(|seq| {
            Ok(pb::LandEvent {
                seq,
                hash: hash_to_proto(&fake_hash(&format!("land{seq}"))),
                change_id: id_to_proto(&ChangeId::derived("land", &seq.to_string())),
                landed_by: (pb::Principal::from(Principal::service("lander"))).into(),
                at: seq as i64,
                reverts_seq: None,
            })
        });
        Ok(Response::new(Box::pin(tokio_stream::iter(events))))
    }

    unimplemented_rpcs! {
        list_repos(pb::ListReposRequest) -> pb::ListReposResponse;
        create_repo(pb::CreateRepoRequest) -> pb::CreateRepoResponse;
        get_change_version(pb::GetChangeVersionRequest) -> pb::GetChangeVersionResponse;
        get_change_info(pb::GetChangeInfoRequest) -> pb::GetChangeInfoResponse;
        path_history(pb::PathHistoryRequest) -> pb::PathHistoryResponse;
        grep(pb::GrepRequest) -> pb::GrepResponse;
        blame(pb::BlameRequest) -> pb::BlameResponse;
        get_tree(pb::GetTreeRequest) -> pb::GetTreeResponse;
        has_objects(pb::HasObjectsRequest) -> pb::HasObjectsResponse;
        diff(pb::DiffRequest) -> pb::DiffResponse;
        merge_base(pb::MergeBaseRequest) -> pb::MergeBaseResponse;
        is_ancestor(pb::IsAncestorRequest) -> pb::IsAncestorResponse;
        create_change_version(pb::CreateChangeVersionRequest) -> pb::CreateChangeVersionResponse;
        merge(pb::MergeRequest) -> pb::MergeResponse;
        resolve(pb::ResolveRequest) -> pb::ResolveResponse;
        list_refs(pb::ListRefsRequest) -> pb::ListRefsResponse;
        move_ref(pb::MoveRefRequest) -> pb::MoveRefResponse;
        cut_release(pb::CutReleaseRequest) -> pb::CutReleaseResponse;
        trunk_range(pb::TrunkRangeRequest) -> pb::TrunkRangeResponse;
        watch_refs(pb::WatchRefsRequest) -> ServiceStream<pb::RefEvent>;
        watch_log(pb::WatchLogRequest) -> ServiceStream<pb::LogEntry>;
        watch_events(pb::WatchEventsRequest) -> ServiceStream<pb::EventEnvelope>;
        get_derived(pb::GetDerivedRequest) -> pb::GetDerivedResponse;
        wait_derived(pb::WaitDerivedRequest) -> pb::WaitDerivedResponse;
    }
}
