//! In-process mock gRPC services for SDK tests.
//!
//! `MockRepo` serves trees keyed by root label, a blob store, a canned log
//! and echoing writes; `MockReview` serves land tickets with scripted
//! status sequences. Every other RPC answers `Unimplemented`.

#![allow(dead_code)]

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use futures::stream::BoxStream;
use rill_proto::convert::{hash_from_proto, hash_to_proto, id_to_proto};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::repo_service_server::{RepoService, RepoServiceServer};
use rill_proto::rill::v1::review_service_server::{ReviewService, ReviewServiceServer};
use rill_sdk::{Client, ClientConfig};
use rill_types::{ChangeId, FileKind, ObjectHash, ObjectKind, Principal, INLINE_THRESHOLD};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Request, Response, Status, Streaming};

type RpcResult<T> = Result<Response<T>, Status>;
type BoxFut<'a, T> = Pin<Box<dyn Future<Output = RpcResult<T>> + Send + 'a>>;

/// A file the mock serves.
#[derive(Clone, Debug)]
pub struct MockFile {
    pub bytes: Bytes,
    pub kind: FileKind,
}

impl MockFile {
    pub fn regular(bytes: impl Into<Bytes>) -> Self {
        MockFile {
            bytes: bytes.into(),
            kind: FileKind::Regular,
        }
    }
}

/// Root label → path → file.
pub type Trees = HashMap<String, BTreeMap<String, MockFile>>;

pub struct MockRepo {
    pub trees: Mutex<Trees>,
    pub blobs: Mutex<HashMap<ObjectHash, Bytes>>,
    pub log: Mutex<Vec<pb::ChangeInfo>>,
    pub trunk: Mutex<(u64, ObjectHash)>,
    pub auth_headers: Mutex<Vec<Option<String>>>,
    pub applied: Mutex<Vec<pb::ApplyEditsRequest>>,
    pub put_requests: Mutex<Vec<usize>>,
    pub rebases: Mutex<Vec<(ObjectHash, ObjectHash)>>,
    pub chunk_size: usize,
}

impl Default for MockRepo {
    fn default() -> Self {
        Self::new()
    }
}

impl MockRepo {
    pub fn new() -> Self {
        MockRepo {
            trees: Mutex::default(),
            blobs: Mutex::default(),
            log: Mutex::default(),
            trunk: Mutex::new((0, ObjectHash::ZERO)),
            auth_headers: Mutex::default(),
            applied: Mutex::default(),
            put_requests: Mutex::default(),
            rebases: Mutex::default(),
            chunk_size: 1000,
        }
    }

    /// Install a tree under a root label (`"trunk"`, a hex hash, `"trunk@3"`).
    pub fn set_tree(&self, label: &str, files: Vec<(&str, MockFile)>) {
        let mut trees = self.trees.lock().unwrap();
        let map = files.into_iter().map(|(p, f)| (p.to_string(), f)).collect();
        trees.insert(label.to_string(), map);
    }

    pub fn set_trunk(&self, seq: u64, hash: ObjectHash) {
        *self.trunk.lock().unwrap() = (seq, hash);
    }

    fn root_label(root: Option<&pb::RootSpec>) -> Result<String, Status> {
        match root.and_then(|r| r.root.as_ref()) {
            Some(pb::root_spec::Root::Ref(name)) => Ok(name.clone()),
            Some(pb::root_spec::Root::VersionHash(h)) => Ok(hex(h)),
            Some(pb::root_spec::Root::TreeHash(h)) => Ok(format!("tree:{}", hex(h))),
            Some(pb::root_spec::Root::TrunkSeq(n)) => Ok(format!("trunk@{n}")),
            None => Err(Status::invalid_argument("missing root")),
        }
    }

    fn tree(&self, root: Option<&pb::RootSpec>) -> Result<BTreeMap<String, MockFile>, Status> {
        let label = Self::root_label(root)?;
        self.trees
            .lock()
            .unwrap()
            .get(&label)
            .cloned()
            .ok_or_else(|| Status::not_found(format!("root {label}")))
    }

    /// Content ref for a file, storing the blob when it is not inline.
    fn content_ref(&self, bytes: &Bytes) -> pb::ContentRef {
        if bytes.len() <= INLINE_THRESHOLD {
            return pb::ContentRef {
                r#ref: Some(pb::content_ref::Ref::Inline(bytes.clone())),
            };
        }
        let hash = ObjectHash::digest(ObjectKind::Blob, bytes);
        self.blobs.lock().unwrap().insert(hash, bytes.clone());
        pb::ContentRef {
            r#ref: Some(pb::content_ref::Ref::Blob(pb::BlobRef {
                hash: hash_to_proto(&hash),
                size: bytes.len() as u64,
            })),
        }
    }

    fn entry(&self, file: &MockFile) -> pb::TreeEntry {
        pb::TreeEntry {
            kind: Some(pb::tree_entry::Kind::File(pb::tree_entry::File {
                content: Some(self.content_ref(&file.bytes)),
                kind: pb::FileKind::from(file.kind) as i32,
            })),
        }
    }

    fn record_auth<T>(&self, req: &Request<T>) {
        let header = req
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        self.auth_headers.lock().unwrap().push(header);
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn fake_hash(seed: &str) -> ObjectHash {
    ObjectHash::digest(ObjectKind::ChangeVersion, seed.as_bytes())
}

pub fn sample_info(seed: &str, seq: Option<u64>) -> pb::ChangeInfo {
    pb::ChangeInfo {
        hash: hash_to_proto(&fake_hash(seed)),
        change_id: id_to_proto(&ChangeId::derived("test", seed)),
        parents: vec![],
        predecessor: None,
        author: Some(pb::Principal::from(Principal::human("alice"))),
        provenance: Some(pb::Provenance::default()),
        intent: None,
        message: format!("change {seed}\n\nbody"),
        created_at: 1_700_000_000_000,
        files_changed: 2,
        has_conflicts: false,
        trunk_seq: seq,
    }
}

macro_rules! unimplemented_rpcs {
    ($($name:ident($req:ty) -> $resp:ty;)*) => {
        $(
            fn $name<'a, 'b>(&'a self, _req: Request<$req>) -> BoxFut<'b, $resp>
            where
                'a: 'b,
                Self: 'b,
            {
                Box::pin(async { Err(Status::unimplemented(stringify!($name))) })
            }
        )*
    };
}

#[tonic::async_trait]
impl RepoService for MockRepo {
    type GetBlobStream = BoxStream<'static, Result<pb::BlobChunk, Status>>;
    type WatchRefsStream = BoxStream<'static, Result<pb::RefEvent, Status>>;
    type WatchTrunkStream = BoxStream<'static, Result<pb::LandEvent, Status>>;
    type WatchLogStream = BoxStream<'static, Result<pb::LogEntry, Status>>;

    async fn get_repo(&self, req: Request<pb::GetRepoRequest>) -> RpcResult<pb::GetRepoResponse> {
        self.record_auth(&req);
        let name = req.into_inner().repo;
        if name == "missing" {
            return Err(Status::not_found("no such repo"));
        }
        Ok(Response::new(pb::GetRepoResponse {
            repo: Some(pb::Repo {
                id: id_to_proto(&rill_types::RepoId::derived("test", &name)),
                name,
                description: Some("mock".into()),
                created_at: 1,
                trunk_head_seq: self.trunk.lock().unwrap().0,
                windows_safe: false,
            }),
        }))
    }

    async fn get_change_versions(
        &self,
        req: Request<pb::GetChangeVersionsRequest>,
    ) -> RpcResult<pb::GetChangeVersionsResponse> {
        let id = hex(&req.into_inner().change_id);
        Ok(Response::new(pb::GetChangeVersionsResponse {
            versions: vec![sample_info(&id, None)],
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
        for (path, file) in &tree {
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
                    entry: Some(entry),
                })
                .collect(),
        }))
    }

    async fn resolve_path(
        &self,
        req: Request<pb::ResolvePathRequest>,
    ) -> RpcResult<pb::ResolvePathResponse> {
        let req = req.into_inner();
        let tree = self.tree(req.root.as_ref())?;
        Ok(Response::new(pb::ResolvePathResponse {
            entry: tree.get(&req.path).map(|f| self.entry(f)),
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
        req: Request<Streaming<pb::PutBlobRequest>>,
    ) -> RpcResult<pb::PutBlobResponse> {
        let mut stream = req.into_inner();
        let mut data = Vec::new();
        let mut messages = 0usize;
        let mut saw_header = false;
        while let Some(msg) = stream.message().await? {
            messages += 1;
            match msg.msg {
                Some(pb::put_blob_request::Msg::Header(h)) => {
                    saw_header = true;
                    if h.repo.is_empty() || h.operation_id.len() != 16 {
                        return Err(Status::invalid_argument("bad header"));
                    }
                }
                Some(pb::put_blob_request::Msg::Data(d)) => data.extend_from_slice(&d),
                None => return Err(Status::invalid_argument("empty message")),
            }
        }
        if !saw_header {
            return Err(Status::invalid_argument("no header"));
        }
        self.put_requests.lock().unwrap().push(messages);
        let bytes = Bytes::from(data);
        let hash = ObjectHash::digest(ObjectKind::Blob, &bytes);
        let size = bytes.len() as u64;
        self.blobs.lock().unwrap().insert(hash, bytes);
        Ok(Response::new(pb::PutBlobResponse {
            content: Some(pb::ContentRef {
                r#ref: Some(pb::content_ref::Ref::Blob(pb::BlobRef {
                    hash: hash_to_proto(&hash),
                    size,
                })),
            }),
        }))
    }

    async fn apply_edits(
        &self,
        req: Request<pb::ApplyEditsRequest>,
    ) -> RpcResult<pb::ApplyEditsResponse> {
        let req = req.into_inner();
        self.applied.lock().unwrap().push(req.clone());
        let mut file_changes = BTreeMap::new();
        for edit in &req.edits {
            let change = match &edit.op {
                Some(pb::edit::Op::SetContent(set)) => {
                    pb::file_change::Change::Set(pb::file_change::Set {
                        content: set.content.clone(),
                        kind: set.kind,
                        copy_from: None,
                    })
                }
                Some(pb::edit::Op::Delete(_)) => {
                    pb::file_change::Change::Delete(pb::file_change::Delete {})
                }
                Some(pb::edit::Op::Patch(p)) => {
                    pb::file_change::Change::Set(pb::file_change::Set {
                        content: Some(pb::ContentRef {
                            r#ref: Some(pb::content_ref::Ref::Inline(Bytes::from(p.clone()))),
                        }),
                        kind: pb::FileKind::Regular as i32,
                        copy_from: None,
                    })
                }
                None => return Err(Status::invalid_argument("edit without op")),
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
            author: Some(pb::Principal::from(Principal::human("alice"))),
            provenance: req.provenance.clone(),
            intent: req.intent.clone(),
            message: req.message.clone(),
            created_at: 42,
            extra: BTreeMap::new(),
        };
        let hash = fake_hash(&format!("apply:{}:{}", req.message, req.edits.len()));
        Ok(Response::new(pb::ApplyEditsResponse {
            hash: hash_to_proto(&hash),
            version: Some(version),
        }))
    }

    async fn rebase(&self, req: Request<pb::RebaseRequest>) -> RpcResult<pb::RebaseResponse> {
        let req = req.into_inner();
        let from = hash_from_proto("hash", &req.hash)?;
        let onto = hash_from_proto("new_parent", &req.new_parent)?;
        self.rebases.lock().unwrap().push((from, onto));
        let new_hash = fake_hash(&format!("rebase:{from}:{onto}"));
        Ok(Response::new(pb::RebaseResponse {
            hash: hash_to_proto(&new_hash),
            version: Some(pb::ChangeVersion {
                change_id: id_to_proto(&ChangeId::derived("rebase", &from.to_hex())),
                parents: vec![req.new_parent],
                predecessor: Some(req.hash),
                subtree_changes: BTreeMap::new(),
                file_changes: BTreeMap::new(),
                author: Some(pb::Principal::from(Principal::human("alice"))),
                provenance: None,
                intent: None,
                message: "rebased".into(),
                created_at: 1,
                extra: BTreeMap::new(),
            }),
        }))
    }

    async fn resolve_ref(
        &self,
        req: Request<pb::ResolveRefRequest>,
    ) -> RpcResult<pb::ResolveRefResponse> {
        let name = req.into_inner().name;
        if name != "trunk" {
            return Err(Status::not_found(format!("ref {name}")));
        }
        let (seq, hash) = *self.trunk.lock().unwrap();
        Ok(Response::new(pb::ResolveRefResponse {
            hash: hash_to_proto(&hash),
            trunk_seq: Some(seq),
        }))
    }

    async fn trunk_head(
        &self,
        req: Request<pb::TrunkHeadRequest>,
    ) -> RpcResult<pb::TrunkHeadResponse> {
        self.record_auth(&req);
        let (seq, hash) = *self.trunk.lock().unwrap();
        Ok(Response::new(pb::TrunkHeadResponse {
            seq,
            hash: hash_to_proto(&hash),
        }))
    }

    async fn watch_trunk(
        &self,
        _req: Request<pb::WatchTrunkRequest>,
    ) -> RpcResult<Self::WatchTrunkStream> {
        let events = (1..=3u64).map(|seq| {
            Ok(pb::LandEvent {
                seq,
                hash: hash_to_proto(&fake_hash(&format!("land{seq}"))),
                change_id: id_to_proto(&ChangeId::derived("land", &seq.to_string())),
                landed_by: Some(pb::Principal::from(Principal::service("lander"))),
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
        trunk_range(pb::TrunkRangeRequest) -> pb::TrunkRangeResponse;
        watch_refs(pb::WatchRefsRequest) -> Self::WatchRefsStream;
        watch_log(pb::WatchLogRequest) -> Self::WatchLogStream;
        get_derived(pb::GetDerivedRequest) -> pb::GetDerivedResponse;
        wait_derived(pb::WaitDerivedRequest) -> pb::WaitDerivedResponse;
    }
}

/// Review service whose land tickets follow a scripted status sequence.
#[derive(Default)]
pub struct MockReview {
    /// Statuses to report, in order; the last one repeats.
    pub script: Mutex<VecDeque<pb::LandStatus>>,
    pub polls: Mutex<usize>,
    pub submitted: Mutex<Vec<ChangeId>>,
    pub stacks: Mutex<Vec<Vec<ChangeId>>>,
    pub land_requests: Mutex<Vec<ChangeId>>,
}

impl MockReview {
    pub fn with_script(statuses: &[pb::LandStatus]) -> Self {
        MockReview {
            script: Mutex::new(statuses.iter().copied().collect()),
            ..Default::default()
        }
    }

    fn ticket(&self, id: &str, change_id: Bytes, status: pb::LandStatus) -> pb::LandTicket {
        pb::LandTicket {
            id: id.to_string(),
            change_id,
            status: status as i32,
            requested_at: 1,
        }
    }

    fn review_state(change_id: Bytes, stack: Vec<Bytes>) -> pb::ReviewState {
        pb::ReviewState {
            change_id,
            versions: vec![hash_to_proto(&fake_hash("v1"))],
            status: pb::ReviewStatus::Open as i32,
            intent: None,
            owner: Some(pb::Principal::from(Principal::human("alice"))),
            votes: vec![],
            threads: vec![],
            risk: pb::RiskClass::Normal as i32,
            risk_reasons: vec![],
            stack,
            landed_seq: None,
            created_at: 1,
            updated_at: 1,
            extra: BTreeMap::new(),
        }
    }
}

#[tonic::async_trait]
impl ReviewService for MockReview {
    type WatchReviewStream = BoxStream<'static, Result<pb::ReviewEvent, Status>>;

    async fn submit_for_review(
        &self,
        req: Request<pb::SubmitForReviewRequest>,
    ) -> RpcResult<pb::SubmitForReviewResponse> {
        let req = req.into_inner();
        let id: ChangeId = rill_proto::convert::id_from_proto("change_id", &req.change_id)?;
        self.submitted.lock().unwrap().push(id);
        Ok(Response::new(pb::SubmitForReviewResponse {
            review: Some(Self::review_state(req.change_id, vec![])),
        }))
    }

    async fn set_stack(
        &self,
        req: Request<pb::SetStackRequest>,
    ) -> RpcResult<pb::SetStackResponse> {
        let req = req.into_inner();
        let ids: Vec<ChangeId> =
            rill_proto::convert::ids_from_proto("change_ids", &req.change_ids)?;
        self.stacks.lock().unwrap().push(ids);
        Ok(Response::new(pb::SetStackResponse {
            reviews: req
                .change_ids
                .iter()
                .map(|id| Self::review_state(id.clone(), req.change_ids.clone()))
                .collect(),
        }))
    }

    async fn request_land(
        &self,
        req: Request<pb::RequestLandRequest>,
    ) -> RpcResult<pb::RequestLandResponse> {
        let req = req.into_inner();
        let id: ChangeId = rill_proto::convert::id_from_proto("change_id", &req.change_id)?;
        self.land_requests.lock().unwrap().push(id);
        Ok(Response::new(pb::RequestLandResponse {
            ticket: Some(self.ticket(
                &format!("ticket-{id}"),
                req.change_id,
                pb::LandStatus::Queued,
            )),
        }))
    }

    async fn get_land_status(
        &self,
        req: Request<pb::GetLandStatusRequest>,
    ) -> RpcResult<pb::GetLandStatusResponse> {
        let ticket = req.into_inner().ticket;
        *self.polls.lock().unwrap() += 1;
        let status = {
            let mut script = self.script.lock().unwrap();
            if script.len() > 1 {
                script.pop_front().unwrap()
            } else {
                script.front().copied().unwrap_or(pb::LandStatus::Landed)
            }
        };
        let landed = status == pb::LandStatus::Landed;
        let change_id = id_to_proto(&ChangeId::derived("ticket", &ticket));
        Ok(Response::new(pb::GetLandStatusResponse {
            ticket: Some(self.ticket(&ticket, change_id, status)),
            seq: landed.then_some(7),
            landed_hash: landed.then(|| hash_to_proto(&fake_hash("landed"))),
            error: (status == pb::LandStatus::Failed).then(|| "boom".to_string()),
        }))
    }

    unimplemented_rpcs! {
        get_review(pb::GetReviewRequest) -> pb::GetReviewResponse;
        list_reviews(pb::ListReviewsRequest) -> pb::ListReviewsResponse;
        vote(pb::VoteRequest) -> pb::VoteResponse;
        comment(pb::CommentRequest) -> pb::CommentResponse;
        reply_thread(pb::ReplyThreadRequest) -> pb::ReplyThreadResponse;
        resolve_thread(pb::ResolveThreadRequest) -> pb::ResolveThreadResponse;
        evaluate_submit(pb::EvaluateSubmitRequest) -> pb::EvaluateSubmitResponse;
        abandon(pb::AbandonRequest) -> pb::AbandonResponse;
        watch_review(pb::WatchReviewRequest) -> Self::WatchReviewStream;
    }
}

/// A running mock server and a client connected to it.
pub struct Harness {
    pub client: Client,
    pub repo: Arc<MockRepo>,
    pub review: Arc<MockReview>,
    pub endpoint: String,
}

impl Harness {
    pub async fn start(repo: MockRepo, review: MockReview, token: Option<&str>) -> Self {
        let repo = Arc::new(repo);
        let review = Arc::new(review);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (repo_s, review_s) = (repo.clone(), review.clone());
        tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(RepoServiceServer::from_arc(repo_s))
                .add_service(ReviewServiceServer::from_arc(review_s))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .unwrap();
        });
        let endpoint = format!("http://{addr}");
        let mut config = ClientConfig::new(endpoint.clone());
        config.token = token.map(str::to_string);
        let client = Client::connect(config).await.unwrap();
        Harness {
            client,
            repo,
            review,
            endpoint,
        }
    }

    pub async fn with_repo(repo: MockRepo) -> Self {
        Self::start(repo, MockReview::default(), None).await
    }
}
