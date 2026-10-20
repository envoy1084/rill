//! End-to-end smoke test: an in-process server on ephemeral ports driven
//! through the generated tonic clients and a raw HTTP client.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::admin_service_client::AdminServiceClient;
use rill_proto::rill::v1::repo_service_client::RepoServiceClient;
use rill_proto::rill::v1::review_service_client::ReviewServiceClient;
use rill_proto::rill::v1::workspace_service_client::WorkspaceServiceClient;
use rill_server::config::{AuthConfig, StaticToken};
use rill_server::{build_state, serve_with_state, AppState, Listeners, ServerConfig};
use rill_types::{ObjectHash, ObjectKind, PrincipalKind};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;
use tonic::metadata::MetadataValue;
use tonic::service::interceptor::InterceptedService;
use tonic::service::Interceptor;
use tonic::transport::Channel;
use tonic::{Code, Request, Status};

/// A running server and how to reach it.
struct TestServer {
    grpc: String,
    http: std::net::SocketAddr,
    shutdown: CancellationToken,
    _state: Arc<AppState>,
    task: tokio::task::JoinHandle<rill_server::Result<()>>,
}

impl TestServer {
    async fn start(config: ServerConfig) -> Self {
        let shutdown = CancellationToken::new();
        let listeners = Listeners::bind(
            "127.0.0.1:0".parse().unwrap(),
            "127.0.0.1:0".parse().unwrap(),
        )
        .await
        .unwrap();
        let (grpc_addr, http) = listeners.addrs().unwrap();
        let state = build_state(config, shutdown.clone()).await.unwrap();
        let task = tokio::spawn(serve_with_state(state.clone(), listeners, shutdown.clone()));
        TestServer {
            grpc: format!("http://{grpc_addr}"),
            http,
            shutdown,
            _state: state,
            task,
        }
    }

    async fn channel(&self) -> Channel {
        let mut last = None;
        for _ in 0..50 {
            match Channel::from_shared(self.grpc.clone())
                .unwrap()
                .connect()
                .await
            {
                Ok(c) => return c,
                Err(e) => {
                    last = Some(e);
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        }
        panic!("could not connect: {last:?}");
    }

    async fn stop(self) {
        self.shutdown.cancel();
        self.task.await.unwrap().unwrap();
    }
}

#[derive(Clone)]
struct Bearer(Option<String>);

impl Interceptor for Bearer {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        if let Some(token) = &self.0 {
            let value = MetadataValue::try_from(format!("Bearer {token}")).unwrap();
            request.metadata_mut().insert("authorization", value);
        }
        Ok(request)
    }
}

type Svc = InterceptedService<Channel, Bearer>;

struct Clients {
    repo: RepoServiceClient<Svc>,
    workspace: WorkspaceServiceClient<Svc>,
    review: ReviewServiceClient<Svc>,
    admin: AdminServiceClient<Svc>,
}

async fn clients(server: &TestServer, token: Option<&str>) -> Clients {
    let channel = server.channel().await;
    let bearer = Bearer(token.map(str::to_owned));
    Clients {
        repo: RepoServiceClient::with_interceptor(channel.clone(), bearer.clone()),
        workspace: WorkspaceServiceClient::with_interceptor(channel.clone(), bearer.clone()),
        review: ReviewServiceClient::with_interceptor(channel.clone(), bearer.clone()),
        admin: AdminServiceClient::with_interceptor(channel, bearer),
    }
}

fn inline(bytes: &[u8]) -> pb::ContentRef {
    pb::ContentRef {
        r#ref: Some(pb::content_ref::Ref::Inline(Bytes::copy_from_slice(bytes))),
    }
}

fn set_edit(path: &str, bytes: &[u8]) -> pb::Edit {
    pb::Edit {
        path: path.into(),
        op: Some(pb::edit::Op::SetContent(pb::SetContent {
            content: Some(inline(bytes)),
            kind: pb::FileKind::Regular as i32,
            copy_from: None,
        })),
    }
}

fn trunk() -> pb::RootSpec {
    pb::RootSpec {
        root: Some(pb::root_spec::Root::Ref("trunk".into())),
    }
}

fn dev_config() -> ServerConfig {
    ServerConfig {
        dev_mode: true,
        ..ServerConfig::default()
    }
}

async fn land(clients: &mut Clients, repo: &str, change_id: Bytes) -> u64 {
    clients
        .review
        .submit_for_review(pb::SubmitForReviewRequest {
            operation_id: Bytes::new(),
            repo: repo.into(),
            change_id: change_id.clone(),
        })
        .await
        .unwrap();
    for (label, value) in [("Verified", 1), ("Review", 2)] {
        clients
            .review
            .vote(pb::VoteRequest {
                operation_id: Bytes::new(),
                repo: repo.into(),
                change_id: change_id.clone(),
                label: label.into(),
                value,
                reason: None,
                version: None,
            })
            .await
            .unwrap();
    }
    let ticket = clients
        .review
        .request_land(pb::RequestLandRequest {
            operation_id: Bytes::new(),
            repo: repo.into(),
            change_id: change_id.clone(),
        })
        .await
        .unwrap()
        .into_inner()
        .ticket
        .unwrap();
    for _ in 0..200 {
        let status = clients
            .review
            .get_land_status(pb::GetLandStatusRequest {
                repo: repo.into(),
                ticket: ticket.id.clone(),
            })
            .await
            .unwrap()
            .into_inner();
        match pb::LandStatus::try_from(status.ticket.as_ref().unwrap().status).unwrap() {
            pb::LandStatus::Landed => return status.seq.unwrap(),
            pb::LandStatus::Failed | pb::LandStatus::Cancelled => {
                panic!("landing failed: {:?}", status.error)
            }
            _ => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }
    panic!("landing did not complete");
}

async fn http_get(addr: std::net::SocketAddr, path: &str) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.unwrap();
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8_lossy(&raw[..split]).to_string();
    let body = raw[split + 4..].to_vec();
    let mut lines = head.lines();
    let status: u16 = lines
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let headers = lines
        .filter_map(|l| l.split_once(": "))
        .map(|(k, v)| (k.to_ascii_lowercase(), v.to_string()))
        .collect();
    (status, headers, body)
}

#[tokio::test]
async fn dev_mode_end_to_end() {
    let server = TestServer::start(dev_config()).await;
    let mut c = clients(&server, None).await;
    let repo = "main";

    let created = c
        .repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: repo.into(),
            description: Some("smoke".into()),
            windows_safe: false,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(created.repo.unwrap().name, "main");

    // Watch trunk before anything lands so the event is observed live.
    let mut trunk_watch = c
        .repo
        .watch_trunk(pb::WatchTrunkRequest {
            repo: repo.into(),
            after_seq: Some(0),
        })
        .await
        .unwrap()
        .into_inner();

    let first = c
        .repo
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Bytes::new(),
            repo: repo.into(),
            base: None,
            edits: vec![
                set_edit("README.md", b"hello\nworld\n"),
                set_edit("src/lib.rs", b"fn a() {}\n"),
            ],
            message: "initial".into(),
            intent: None,
            provenance: None,
            change_id: None,
        })
        .await
        .unwrap()
        .into_inner();
    let change_id = first.version.as_ref().unwrap().change_id.clone();
    assert!(first.version.unwrap().parents.is_empty());

    let seq = land(&mut c, repo, change_id.clone()).await;
    assert_eq!(seq, 1);

    let landed = tokio::time::timeout(Duration::from_secs(5), trunk_watch.message())
        .await
        .expect("trunk event")
        .unwrap()
        .unwrap();
    assert_eq!(landed.seq, 1);
    assert_eq!(landed.change_id, change_id);

    let listing = c
        .repo
        .list_dir(pb::ListDirRequest {
            repo: repo.into(),
            root: Some(trunk()),
            path: String::new(),
        })
        .await
        .unwrap()
        .into_inner();
    let names: Vec<_> = listing.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["README.md", "src"]);

    // A second change on top of trunk, amended through a patch.
    let second = c
        .repo
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Bytes::new(),
            repo: repo.into(),
            base: Some(pb::EditBase {
                base: Some(pb::edit_base::Base::Root(trunk())),
            }),
            edits: vec![pb::Edit {
                path: "README.md".into(),
                op: Some(pb::edit::Op::Patch(
                    "--- a/README.md\n+++ b/README.md\n@@ -1,2 +1,2 @@\n hello\n-world\n+there\n"
                        .into(),
                )),
            }],
            message: "patch readme".into(),
            intent: None,
            provenance: None,
            change_id: None,
        })
        .await
        .unwrap()
        .into_inner();

    let diff = c
        .repo
        .diff(pb::DiffRequest {
            repo: repo.into(),
            base: Some(trunk()),
            target: Some(pb::RootSpec {
                root: Some(pb::root_spec::Root::VersionHash(second.hash.clone())),
            }),
            paths: vec![],
            format: pb::DiffFormat::Structured as i32,
            context_lines: 0,
            limit: 0,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(diff.files.len(), 1);
    assert_eq!(diff.files[0].path, "README.md");
    let hunk = &diff.files[0].hunks[0];
    let ops: Vec<i32> = hunk.lines.iter().map(|l| l.op).collect();
    assert!(ops.contains(&(pb::diff_line::Op::Delete as i32)));
    assert!(ops.contains(&(pb::diff_line::Op::Insert as i32)));

    let blame = c
        .repo
        .blame(pb::BlameRequest {
            repo: repo.into(),
            hash: second.hash.clone(),
            path: "README.md".into(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(blame.lines.len(), 2);
    assert_eq!(blame.lines[0].version, first.hash);
    assert_eq!(blame.lines[0].trunk_seq, Some(1));
    assert_eq!(blame.lines[1].version, second.hash);
    assert_eq!(blame.lines[1].text, Bytes::from_static(b"there\n"));

    // Grep is served from the search index; the root is indexed on demand.
    let found = c
        .repo
        .grep(pb::GrepRequest {
            repo: repo.into(),
            root: Some(pb::RootSpec {
                root: Some(pb::root_spec::Root::VersionHash(second.hash.clone())),
            }),
            pattern: "THERE".into(),
            case_insensitive: true,
            path_globs: vec!["*.md".into()],
            max_results: 10,
            context_lines: 1,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(found.hits.len(), 1, "{found:?}");
    assert_eq!(found.hits[0].path, "README.md");
    assert_eq!(found.hits[0].line_no, 2);
    assert_eq!(found.hits[0].before.len(), 1);
    assert!(!found.truncated);
    let none = c
        .repo
        .grep(pb::GrepRequest {
            repo: repo.into(),
            root: None,
            pattern: "THERE".into(),
            case_insensitive: false,
            path_globs: vec![],
            max_results: 0,
            context_lines: 0,
        })
        .await
        .unwrap()
        .into_inner();
    assert!(none.hits.is_empty());

    // Workspace flow: upload a blob, snapshot the tree it belongs to, and
    // turn the snapshot into a change.
    let big = vec![b'x'; 300];
    let put = c
        .repo
        .put_blob(tokio_stream::iter(vec![
            pb::PutBlobRequest {
                msg: Some(pb::put_blob_request::Msg::Header(pb::PutBlobHeader {
                    operation_id: Bytes::new(),
                    repo: repo.into(),
                    size: Some(big.len() as u64),
                    expected_hash: Some(Bytes::copy_from_slice(
                        ObjectHash::digest(ObjectKind::Blob, &big).as_bytes(),
                    )),
                })),
            },
            pb::PutBlobRequest {
                msg: Some(pb::put_blob_request::Msg::Data(Bytes::from(big.clone()))),
            },
        ]))
        .await
        .unwrap()
        .into_inner();
    let blob_ref = put.content.unwrap();
    let blob_hash = match &blob_ref.r#ref {
        Some(pb::content_ref::Ref::Blob(b)) => ObjectHash::from_slice(&b.hash).unwrap(),
        other => panic!("expected a stored blob, got {other:?}"),
    };

    let ws = c
        .workspace
        .create_workspace(pb::CreateWorkspaceRequest {
            operation_id: Bytes::new(),
            repo: repo.into(),
            name: "scratch".into(),
            base: None,
            sparse: None,
        })
        .await
        .unwrap()
        .into_inner()
        .workspace
        .unwrap();
    assert_eq!(ws.base, first.hash);

    // The snapshot tree is built server-side through ApplyEdits (a draft
    // version whose root tree we reuse), as a client's tree builder would.
    let draft = c
        .repo
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Bytes::new(),
            repo: repo.into(),
            base: Some(pb::EditBase {
                base: Some(pb::edit_base::Base::Root(trunk())),
            }),
            edits: vec![pb::Edit {
                path: "data.bin".into(),
                op: Some(pb::edit::Op::SetContent(pb::SetContent {
                    content: Some(blob_ref.clone()),
                    kind: pb::FileKind::Regular as i32,
                    copy_from: None,
                })),
            }],
            message: "draft".into(),
            intent: None,
            provenance: None,
            change_id: None,
        })
        .await
        .unwrap()
        .into_inner();
    let draft_root = c
        .repo
        .get_derived(pb::GetDerivedRequest {
            repo: repo.into(),
            kind: "root@1".into(),
            hash: draft.hash.clone(),
        })
        .await
        .unwrap()
        .into_inner()
        .data;
    // Derived records are postcard: a one-byte version then the 32-byte hash.
    let tree_hash = Bytes::copy_from_slice(&draft_root[draft_root.len() - 32..]);
    c.workspace
        .snapshot(pb::SnapshotRequest {
            operation_id: Bytes::new(),
            repo: repo.into(),
            workspace: ws.id.clone(),
            tree_hash,
        })
        .await
        .unwrap();
    let new_change = c
        .workspace
        .new_change(pb::NewChangeRequest {
            operation_id: Bytes::new(),
            repo: repo.into(),
            workspace: ws.id.clone(),
            message: "from workspace".into(),
            intent: None,
            provenance: None,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(new_change.change_id, ws.working_change);
    let version = new_change.version.unwrap();
    assert_eq!(version.parents, vec![first.hash.clone()]);
    assert!(version.file_changes.contains_key("data.bin"));
    let ops = c
        .workspace
        .list_operations(pb::ListOperationsRequest {
            repo: repo.into(),
            workspace: ws.id.clone(),
            limit: 0,
            before: None,
        })
        .await
        .unwrap()
        .into_inner()
        .operations;
    assert_eq!(ops.len(), 3);

    // Object protocol.
    let (status, headers, body) = http_get(
        server.http,
        &format!("/o/{repo}/blob/{}", blob_hash.to_hex()),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body, big);
    let etag = headers
        .iter()
        .find(|(k, _)| k == "etag")
        .map(|(_, v)| v.clone());
    assert_eq!(
        etag.as_deref(),
        Some(format!("\"{}\"", blob_hash.to_hex()).as_str())
    );
    assert!(headers
        .iter()
        .any(|(k, v)| k == "cache-control" && v.contains("immutable")));
    let (status, _, _) = http_get(server.http, "/healthz").await;
    assert_eq!(status, 200);
    let (status, _, _) = http_get(server.http, "/o/main/blob/zz").await;
    assert_eq!(status, 400);

    let me = c
        .admin
        .who_am_i(pb::WhoAmIRequest {})
        .await
        .unwrap()
        .into_inner();
    assert_eq!(me.principal.unwrap().name, "dev");
    let health = c
        .admin
        .health(pb::HealthRequest {})
        .await
        .unwrap()
        .into_inner();
    assert!(health.ok);

    server.stop().await;
}

#[tokio::test]
async fn static_auth_denies_unrelated_principal() {
    let auth = AuthConfig::Static {
        tokens: vec![
            StaticToken {
                token: "alice-token".into(),
                principal_name: "alice".into(),
                kind: PrincipalKind::Human,
                teams: vec![],
            },
            StaticToken {
                token: "mallory-token".into(),
                principal_name: "mallory".into(),
                kind: PrincipalKind::Agent,
                teams: vec![],
            },
        ],
    };
    let config = ServerConfig {
        auth,
        ..ServerConfig::default()
    };
    let server = TestServer::start(config).await;

    // No token at all: unauthenticated.
    let mut anon = clients(&server, None).await;
    let err = anon
        .repo
        .list_repos(pb::ListReposRequest {
            limit: 0,
            page_token: None,
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), Code::Unauthenticated);

    let mut alice = clients(&server, Some("alice-token")).await;
    alice
        .repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: "private".into(),
            description: None,
            windows_safe: false,
        })
        .await
        .unwrap();
    let relations = alice
        .admin
        .list_relations(pb::ListRelationsRequest {
            object: "repo:private".into(),
            relation: None,
        })
        .await
        .unwrap()
        .into_inner()
        .relations;
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0].relation, "owner");

    let mut mallory = clients(&server, Some("mallory-token")).await;
    let err = mallory
        .repo
        .list_dir(pb::ListDirRequest {
            repo: "private".into(),
            root: None,
            path: String::new(),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), Code::PermissionDenied);
    let visible = mallory
        .repo
        .list_repos(pb::ListReposRequest {
            limit: 0,
            page_token: None,
        })
        .await
        .unwrap()
        .into_inner();
    assert!(visible.repos.is_empty());

    // Granting a reader relation opens reads but not writes.
    alice
        .admin
        .add_relation(pb::AddRelationRequest {
            operation_id: Bytes::new(),
            relation: Some(pb::Relation {
                object: "repo:private".into(),
                relation: "reader".into(),
                subject: format!("principal:{}", rill_types::Principal::agent("mallory").id),
            }),
        })
        .await
        .unwrap();
    mallory
        .repo
        .list_dir(pb::ListDirRequest {
            repo: "private".into(),
            root: None,
            path: String::new(),
        })
        .await
        .unwrap();
    let err = mallory
        .repo
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Bytes::new(),
            repo: "private".into(),
            base: None,
            edits: vec![set_edit("x", b"y")],
            message: "nope".into(),
            intent: None,
            provenance: None,
            change_id: None,
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), Code::PermissionDenied);

    // Object protocol honours the same rules.
    let mut stream = TcpStream::connect(server.http).await.unwrap();
    stream
        .write_all(b"GET /o/private/blob/00 HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).await.unwrap();
    assert!(raw.starts_with("HTTP/1.1 401"), "{raw}");

    server.stop().await;
}

#[tokio::test]
async fn commits_enforce_path_rules() {
    let server = TestServer::start(dev_config()).await;
    let mut c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: "win".into(),
            description: None,
            windows_safe: true,
        })
        .await
        .unwrap();
    let edit = |path: &'static str| {
        let mut repo = c.repo.clone();
        async move {
            repo.apply_edits(pb::ApplyEditsRequest {
                operation_id: Bytes::new(),
                repo: "win".into(),
                base: None,
                edits: vec![set_edit(path, b"x\n")],
                message: path.into(),
                intent: None,
                provenance: None,
                change_id: None,
            })
            .await
        }
    };
    assert_eq!(
        edit("src/CON.txt").await.unwrap_err().code(),
        Code::InvalidArgument
    );
    assert_eq!(
        edit(".rill/checkout.toml").await.unwrap_err().code(),
        Code::InvalidArgument
    );
    edit("README.md").await.unwrap();
    let first = c
        .repo
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Bytes::new(),
            repo: "win".into(),
            base: None,
            edits: vec![set_edit("README.md", b"x\n"), set_edit("readme.md", b"y\n")],
            message: "case".into(),
            intent: None,
            provenance: None,
            change_id: None,
        })
        .await;
    assert_eq!(first.unwrap_err().code(), Code::InvalidArgument);
}

#[tokio::test]
async fn server_stamps_authorship_and_attests_commits() {
    let server = TestServer::start(dev_config()).await;
    let mut c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: "auth".into(),
            description: None,
            windows_safe: false,
        })
        .await
        .unwrap();

    // A client claiming to be someone else, at another time, sponsored and
    // conflict-resolved by whoever it likes.
    let mut claimed = rill_types::ChangeVersion {
        change_id: rill_types::ChangeId::new(),
        parents: Vec::new(),
        predecessor: None,
        subtree_changes: Default::default(),
        file_changes: Default::default(),
        author: rill_types::Principal::human("alice"),
        provenance: rill_types::Provenance {
            model: Some("model-x".into()),
            sponsor: Some(rill_types::Principal::human("ceo")),
            resolved_by: Some("human".into()),
            ..Default::default()
        },
        intent: None,
        message: "spoof".into(),
        created_at: rill_types::Timestamp::from_millis(1),
        extra: Default::default(),
    };
    claimed.file_changes.insert(
        rill_types::RepoPathBuf::new("a.txt").unwrap(),
        rill_types::FileChange::set(
            rill_types::ContentRef::inline(b"a\n".to_vec()),
            rill_types::FileKind::Regular,
        ),
    );
    let created = c
        .repo
        .create_change_version(pb::CreateChangeVersionRequest {
            operation_id: Bytes::new(),
            repo: "auth".into(),
            version: Some(pb::ChangeVersion::from(claimed)),
            signatures: Vec::new(),
        })
        .await
        .unwrap()
        .into_inner();

    let got = c
        .repo
        .get_change_version(pb::GetChangeVersionRequest {
            repo: "auth".into(),
            hash: created.hash.clone(),
        })
        .await
        .unwrap()
        .into_inner();
    let stored = rill_types::ChangeVersion::try_from(got.version.unwrap()).unwrap();
    assert_eq!(stored.author.name, "dev");
    assert!(stored.created_at.millis() > 1);
    assert_eq!(stored.provenance.model.as_deref(), Some("model-x"));
    assert!(stored.provenance.sponsor.is_none() && stored.provenance.resolved_by.is_none());

    let hash = ObjectHash::from_slice(&created.hash).unwrap();
    let health = c
        .admin
        .health(pb::HealthRequest {})
        .await
        .unwrap()
        .into_inner();
    let attestation = got
        .signatures
        .into_iter()
        .map(|s| rill_types::Signature::try_from(s).unwrap())
        .find(|s| s.signer.name == "rill-server")
        .expect("the server attests every commit");
    assert_eq!(hex::encode(&attestation.public_key), health.attestation_key);
    assert!(rill_policy::verify_signature(&attestation, &hash));
}
