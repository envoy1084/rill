//! Binding listeners and running the gRPC and HTTP servers.

use std::net::SocketAddr;
use std::sync::Arc;

use rill_proto::rill::v1::admin_service_server::AdminServiceServer;
use rill_proto::rill::v1::intent_service_server::IntentServiceServer;
use rill_proto::rill::v1::lander_service_server::LanderServiceServer;
use rill_proto::rill::v1::repo_service_server::RepoServiceServer;
use rill_proto::rill::v1::review_service_server::ReviewServiceServer;
use rill_proto::rill::v1::workspace_service_server::WorkspaceServiceServer;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tonic::server::NamedService;
use tonic::transport::server::TcpIncoming;
use tower::Layer;

use crate::auth::{AuthLayer, AuthService};
use crate::error::{Error, Result};
use crate::objects;
use crate::services::{AdminSvc, IntentSvc, LanderSvc, RepoSvc, ReviewSvc, WorkspaceSvc};
use crate::state::AppState;

// Tonic routes by service name, which the auth wrapper must forward.
impl<S: NamedService> NamedService for AuthService<S> {
    const NAME: &'static str = S::NAME;
}

/// The two listeners a server runs on.
pub struct Listeners {
    /// gRPC API.
    pub grpc: TcpListener,
    /// Object protocol and health.
    pub http: TcpListener,
}

impl Listeners {
    /// Bind both addresses. Port 0 picks a free port; use
    /// [`Listeners::addrs`] to learn what was chosen.
    pub async fn bind(grpc: SocketAddr, http: SocketAddr) -> Result<Self> {
        Ok(Listeners {
            grpc: TcpListener::bind(grpc).await?,
            http: TcpListener::bind(http).await?,
        })
    }

    /// The bound `(grpc, http)` addresses.
    pub fn addrs(&self) -> Result<(SocketAddr, SocketAddr)> {
        Ok((self.grpc.local_addr()?, self.http.local_addr()?))
    }
}

/// Serve the gRPC API and the object protocol on `listeners` until
/// `shutdown` fires, then drain in-flight requests.
pub async fn serve_with_state(
    state: Arc<AppState>,
    listeners: Listeners,
    shutdown: CancellationToken,
) -> Result<()> {
    let (grpc_addr, http_addr) = listeners.addrs()?;
    tracing::info!(%grpc_addr, %http_addr, dev_mode = state.dev_mode(), "rill server listening");

    tokio::spawn(crate::workers::sweep_idempotency(
        state.stores.clone(),
        shutdown.clone(),
    ));
    let grpc = tokio::spawn(serve_grpc(state.clone(), listeners.grpc, shutdown.clone()));
    let http = tokio::spawn(serve_http(state, listeners.http, shutdown.clone()));

    let (grpc, http) = tokio::join!(grpc, http);
    let outcomes = [
        grpc.map_err(|e| Error::Transport(format!("gRPC task: {e}")))?,
        http.map_err(|e| Error::Transport(format!("HTTP task: {e}")))?,
    ];
    for outcome in outcomes {
        outcome?;
    }
    tracing::info!("rill server stopped");
    Ok(())
}

async fn serve_grpc(
    state: Arc<AppState>,
    listener: TcpListener,
    shutdown: CancellationToken,
) -> Result<()> {
    let auth = AuthLayer::new(state.clone());
    let reflection = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(rill_proto::FILE_DESCRIPTOR_SET)
        .build_v1()
        .map_err(|e| Error::Transport(format!("reflection: {e}")))?;
    let (health_reporter, health) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<RepoServiceServer<RepoSvc>>()
        .await;
    health_reporter
        .set_serving::<WorkspaceServiceServer<WorkspaceSvc>>()
        .await;
    health_reporter
        .set_serving::<ReviewServiceServer<ReviewSvc>>()
        .await;
    health_reporter
        .set_serving::<IntentServiceServer<IntentSvc>>()
        .await;
    health_reporter
        .set_serving::<LanderServiceServer<LanderSvc>>()
        .await;
    health_reporter
        .set_serving::<AdminServiceServer<AdminSvc>>()
        .await;

    tonic::transport::Server::builder()
        .add_service(reflection)
        .add_service(health)
        .add_service(auth.layer(RepoServiceServer::new(RepoSvc::new(state.clone()))))
        .add_service(auth.layer(WorkspaceServiceServer::new(WorkspaceSvc::new(
            state.clone(),
        ))))
        .add_service(auth.layer(ReviewServiceServer::new(ReviewSvc::new(state.clone()))))
        .add_service(auth.layer(IntentServiceServer::new(IntentSvc::new(state.clone()))))
        .add_service(auth.layer(LanderServiceServer::new(LanderSvc::new(state.clone()))))
        .add_service(auth.layer(AdminServiceServer::new(AdminSvc::new(state))))
        .serve_with_incoming_shutdown(TcpIncoming::from(listener), shutdown.cancelled_owned())
        .await
        .map_err(|e| Error::Transport(format!("gRPC server: {e}")))
}

async fn serve_http(
    state: Arc<AppState>,
    listener: TcpListener,
    shutdown: CancellationToken,
) -> Result<()> {
    let router = objects::router(state);
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown.cancelled_owned())
        .await
        .map_err(|e| Error::Transport(format!("HTTP server: {e}")))
}
