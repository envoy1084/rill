//! A small in-process mock `RepoService`, in the style of the SDK's test
//! harness: one tree per root label, inline or blob content, a canned log.
//! Every other RPC answers `Unimplemented`.

#![allow(dead_code)]

use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use futures::stream::BoxStream;
use rill_mcp::{McpServer, ToolContext};
use rill_proto::convert::{hash_from_proto, hash_to_proto, id_to_proto};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::repo_service_server::{RepoService, RepoServiceServer};
use rill_sdk::{Client, ClientConfig};
use rill_types::{ChangeId, ObjectHash, ObjectKind, Principal, INLINE_THRESHOLD};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Request, Response, Status, Streaming};

type RpcResult<T> = Result<Response<T>, Status>;
type BoxFut<'a, T> = Pin<Box<dyn Future<Output = RpcResult<T>> + Send + 'a>>;

/// Root label -> path -> bytes.
pub type Trees = HashMap<String, BTreeMap<String, Bytes>>;

#[derive(Default)]
pub struct MockRepo {
    pub trees: Mutex<Trees>,
    pub blobs: Mutex<HashMap<ObjectHash, Bytes>>,
    pub log: Mutex<Vec<pb::ChangeInfo>>,
}

impl MockRepo {
    pub fn set_tree(&self, label: &str, files: Vec<(&str, &[u8])>) {
        let map = files
            .into_iter()
            .map(|(p, b)| (p.to_string(), Bytes::copy_from_slice(b)))
            .collect();
        self.trees.lock().unwrap().insert(label.to_string(), map);
    }

    fn root_label(root: Option<&pb::RootSpec>) -> Result<String, Status> {
        match root.and_then(|r| r.root.as_ref()) {
            Some(pb::root_spec::Root::Ref(name)) => Ok(name.clone()),
            Some(pb::root_spec::Root::VersionHash(h)) => Ok(hex::encode(h)),
            Some(pb::root_spec::Root::TreeHash(h)) => Ok(format!("tree:{}", hex::encode(h))),
            Some(pb::root_spec::Root::TrunkSeq(n)) => Ok(format!("trunk@{n}")),
            None => Err(Status::invalid_argument("missing root")),
        }
    }

    fn tree(&self, root: Option<&pb::RootSpec>) -> Result<BTreeMap<String, Bytes>, Status> {
        let label = Self::root_label(root)?;
        self.trees
            .lock()
            .unwrap()
            .get(&label)
            .cloned()
            .ok_or_else(|| Status::not_found(format!("root {label}")))
    }

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

    fn entry(&self, bytes: &Bytes) -> pb::TreeEntry {
        pb::TreeEntry {
            kind: Some(pb::tree_entry::Kind::File(pb::tree_entry::File {
                content: Some(self.content_ref(bytes)),
                kind: pb::FileKind::Regular as i32,
            })),
        }
    }
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
        let name = req.into_inner().repo;
        Ok(Response::new(pb::GetRepoResponse {
            repo: Some(pb::Repo {
                id: id_to_proto(&rill_types::RepoId::derived("test", &name)),
                name,
                description: Some("mock".into()),
                created_at: 1,
                trunk_head_seq: 3,
                windows_safe: false,
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
        trunk_head(pb::TrunkHeadRequest) -> pb::TrunkHeadResponse;
        trunk_range(pb::TrunkRangeRequest) -> pb::TrunkRangeResponse;
        watch_trunk(pb::WatchTrunkRequest) -> Self::WatchTrunkStream;
        watch_refs(pb::WatchRefsRequest) -> Self::WatchRefsStream;
        watch_log(pb::WatchLogRequest) -> Self::WatchLogStream;
        get_derived(pb::GetDerivedRequest) -> pb::GetDerivedResponse;
        wait_derived(pb::WaitDerivedRequest) -> pb::WaitDerivedResponse;
    }
}

/// A running mock and an MCP server connected to it.
pub struct Harness {
    pub repo: Arc<MockRepo>,
    pub server: McpServer,
}

impl Harness {
    pub async fn start(repo: MockRepo) -> Self {
        let repo = Arc::new(repo);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let svc = repo.clone();
        tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(RepoServiceServer::from_arc(svc))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .unwrap();
        });
        let client = Client::connect(ClientConfig::new(format!("http://{addr}")))
            .await
            .unwrap();
        let server = McpServer::new(ToolContext::new(client, Some("main".into())));
        Harness { repo, server }
    }
}
