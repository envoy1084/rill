//! End-to-end tests: an in-process server on ephemeral ports driven
//! through the generated tonic clients and a raw HTTP client.

mod agents;
mod auth;
mod cluster;
mod commits;
mod end_to_end;
mod events;
mod history;
mod identity;
mod killswitch;
mod kubernetes;
mod orgs;
mod replicas;
mod retries;
mod search;
mod shutdown;
mod signin;
mod telemetry;
mod workspaces;

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use rill_api::{build_state, serve_with_state, AppState, Listeners, ServerConfig};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::admin_service_client::AdminServiceClient;
use rill_proto::rill::v1::repo_service_client::RepoServiceClient;
use rill_proto::rill::v1::review_service_client::ReviewServiceClient;
use rill_proto::rill::v1::workspace_service_client::WorkspaceServiceClient;
use rill_types::{ObjectHash, ObjectKind};
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
    state: Arc<AppState>,
    task: tokio::task::JoinHandle<rill_api::Result<()>>,
}

impl TestServer {
    async fn start(config: ServerConfig) -> Self {
        let shutdown = CancellationToken::new();
        let state = build_state(config, shutdown.clone()).await.unwrap();
        Self::serve(state, shutdown).await
    }

    /// A server over `stores`, which other test servers may share.
    async fn start_on(config: ServerConfig, stores: rill_store::Stores) -> Self {
        let shutdown = CancellationToken::new();
        let state = AppState::with_stores(config, stores, shutdown.clone())
            .await
            .unwrap();
        state.repos.warm().await.unwrap();
        Self::serve(state, shutdown).await
    }

    async fn serve(state: Arc<AppState>, shutdown: CancellationToken) -> Self {
        let listeners = Listeners::bind(
            "127.0.0.1:0".parse().unwrap(),
            "127.0.0.1:0".parse().unwrap(),
        )
        .await
        .unwrap();
        let (grpc_addr, http) = listeners.addrs().unwrap();
        let task = tokio::spawn(serve_with_state(state.clone(), listeners, shutdown.clone()));
        TestServer {
            grpc: format!("http://{grpc_addr}"),
            http,
            shutdown,
            state,
            task,
        }
    }

    /// A token the server signed for `principal`, as a sign-in would issue.
    fn token_for(&self, principal: &rill_types::Principal) -> String {
        let issuer = self.state.identity.issuer().unwrap();
        let ttl = issuer.lifetimes().max_for(principal.kind);
        let claims = rill_policy::TokenClaims::new("default", rill_types::Timestamp::now(), ttl);
        issuer.issue(principal, &claims, None).unwrap()
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
    clients_at(&server.grpc, token).await
}

/// A channel to `endpoint`, retried while the server starts.
async fn connect(endpoint: &str) -> Channel {
    let mut last = None;
    for _ in 0..50 {
        match Channel::from_shared(endpoint.to_string())
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
    panic!("could not connect to {endpoint}: {last:?}");
}

async fn clients_at(endpoint: &str, token: Option<&str>) -> Clients {
    let channel = connect(endpoint).await;
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
