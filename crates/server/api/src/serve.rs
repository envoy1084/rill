//! Binding listeners and running the gRPC and HTTP servers.

use std::net::SocketAddr;
use std::sync::Arc;

use rill_proto::rill::v1::access_service_server::AccessServiceServer;
use rill_proto::rill::v1::admin_service_server::AdminServiceServer;
use rill_proto::rill::v1::audit_service_server::AuditServiceServer;
use rill_proto::rill::v1::identity_service_server::IdentityServiceServer;
use rill_proto::rill::v1::intent_service_server::IntentServiceServer;
use rill_proto::rill::v1::lander_service_server::LanderServiceServer;
use rill_proto::rill::v1::protection_service_server::ProtectionServiceServer;
use rill_proto::rill::v1::repo_service_server::RepoServiceServer;
use rill_proto::rill::v1::review_service_server::ReviewServiceServer;
use rill_proto::rill::v1::secret_service_server::SecretServiceServer;
use rill_proto::rill::v1::session_service_server::SessionServiceServer;
use rill_proto::rill::v1::workspace_service_server::WorkspaceServiceServer;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tonic::server::NamedService;
use tonic::transport::server::TcpIncoming;
use tower::Layer;

use crate::auth::{AuthLayer, AuthService};
use crate::objects;
use rill_reapi_proto::bytestream::byte_stream_server::ByteStreamServer;
use rill_reapi_proto::reapi::action_cache_server::ActionCacheServer;
use rill_reapi_proto::reapi::capabilities_server::CapabilitiesServer;
use rill_reapi_proto::reapi::content_addressable_storage_server::ContentAddressableStorageServer;
use rill_reapi_proto::reapi::execution_server::ExecutionServer;
use rill_reapi_proto::remoteworker::operation_queue_server::OperationQueueServer;

use crate::services::{
    AccessSvc, AdminSvc, AuditSvc, ExecSvc, IdentitySvc, IntentSvc, LanderSvc, ProtectionSvc,
    RepoSvc, ReviewSvc, SecretSvc, SessionSvc, WorkspaceSvc,
};
use rill_runtime::error::{Error, Result};
use rill_runtime::state::AppState;

/// Largest gRPC request message: a change version (8 MiB) with room for
/// its envelope; blobs stream in smaller pieces.
pub const MAX_REQUEST_BYTES: usize = 16 << 20;
/// Largest gRPC response message; listings and diffs stay well under it.
pub const MAX_RESPONSE_BYTES: usize = 64 << 20;
/// Requests one connection may have in flight.
pub const MAX_CONCURRENT_PER_CONNECTION: usize = 1024;

/// A generated service with the message size limits.
macro_rules! limits {
    ($svc:expr) => {
        $svc.max_decoding_message_size(MAX_REQUEST_BYTES)
            .max_encoding_message_size(MAX_RESPONSE_BYTES)
    };
}

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

    let roles = state.config.roles;
    tokio::spawn(rill_runtime::telemetry::sample_forever(
        state.clone(),
        shutdown.clone(),
    ));
    tokio::spawn(rill_runtime::identity::follow(
        state.identity.clone(),
        shutdown.clone(),
    ));
    if roles.workers {
        tokio::spawn(rill_runtime::audit::worker::run(
            state.clone(),
            shutdown.clone(),
        ));
        tokio::spawn(rill_runtime::identity::maintain_forever(
            state.identity.clone(),
            shutdown.clone(),
        ));
        tokio::spawn(rill_runtime::workers::sweep_idempotency(
            state.stores.clone(),
            shutdown.clone(),
        ));
        tokio::spawn(reap_execution(state.clone(), shutdown.clone()));
        rill_runtime::workers::spawn_local_workers(&state, &shutdown)?;
        tokio::spawn(rill_runtime::workers::discover_repos(
            state.clone(),
            shutdown.clone(),
        ));
    }
    tokio::spawn(sweep_rate_limits(state.clone(), shutdown.clone()));
    let grpc = roles
        .api
        .then(|| tokio::spawn(serve_grpc(state.clone(), listeners.grpc, shutdown.clone())));
    let http = tokio::spawn(serve_http(state, listeners.http, shutdown.clone()));

    let mut outcomes = vec![http
        .await
        .map_err(|e| Error::Transport(format!("HTTP task: {e}")))?];
    if let Some(grpc) = grpc {
        outcomes.push(
            grpc.await
                .map_err(|e| Error::Transport(format!("gRPC task: {e}")))?,
        );
    }
    for outcome in outcomes {
        outcome?;
    }
    tracing::info!("rill server stopped");
    Ok(())
}

async fn sweep_rate_limits(state: Arc<AppState>, shutdown: CancellationToken) {
    let Some(limits) = &state.rate_limits else {
        return;
    };
    let mut tick = tokio::time::interval(rill_runtime::rate::SWEEP_EVERY);
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => return,
            _ = tick.tick() => limits.sweep(),
        }
    }
}

/// How often actions are taken back from workers that stopped reporting.
const EXEC_REAP_EVERY: std::time::Duration = std::time::Duration::from_secs(10);

async fn reap_execution(state: Arc<AppState>, shutdown: CancellationToken) {
    if !state.config.exec.enabled && !state.config.verify.enabled {
        return;
    }
    let mut tick = tokio::time::interval(EXEC_REAP_EVERY);
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => return,
            _ = tick.tick() => {
                if let Err(e) = state.exec.reap().await {
                    tracing::warn!(error = %e, "reaping remote execution");
                }
            }
        }
    }
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
    set_health(&health_reporter, true).await;
    let reporter = health_reporter.clone();
    let draining = state.lifecycle.draining.clone();
    tokio::spawn(async move {
        draining.cancelled().await;
        set_health(&reporter, false).await;
    });

    let mut builder = tonic::transport::Server::builder()
        .concurrency_limit_per_connection(MAX_CONCURRENT_PER_CONNECTION)
        .layer(rill_runtime::telemetry::TelemetryLayer::new(
            state.metrics.clone(),
        ));
    if let Some(tls) = &state.config.tls {
        builder = builder
            .tls_config(crate::tls::grpc(tls)?)
            .map_err(|e| Error::Config(format!("tls: {e}")))?;
    }
    let exec = ExecSvc::new(state.clone());
    builder
        .add_service(reflection)
        .add_service(health)
        // Signing in needs no token; each RPC checks what it is given.
        .add_service(limits!(SessionServiceServer::new(SessionSvc::new(
            state.clone()
        ))))
        .add_service(auth.layer(limits!(RepoServiceServer::new(RepoSvc::new(state.clone())))))
        .add_service(
            auth.layer(limits!(WorkspaceServiceServer::new(WorkspaceSvc::new(
                state.clone()
            )))),
        )
        .add_service(auth.layer(limits!(ReviewServiceServer::new(ReviewSvc::new(
            state.clone()
        )))))
        .add_service(auth.layer(limits!(IntentServiceServer::new(IntentSvc::new(
            state.clone()
        )))))
        .add_service(auth.layer(limits!(LanderServiceServer::new(LanderSvc::new(
            state.clone()
        )))))
        .add_service(
            auth.layer(limits!(IdentityServiceServer::new(IdentitySvc::new(
                state.clone()
            )))),
        )
        .add_service(auth.layer(limits!(AccessServiceServer::new(AccessSvc::new(
            state.clone()
        )))))
        .add_service(auth.layer(limits!(AuditServiceServer::new(AuditSvc::new(
            state.clone()
        )))))
        .add_service(
            auth.layer(limits!(ProtectionServiceServer::new(ProtectionSvc::new(
                state.clone()
            )))),
        )
        .add_service(auth.layer(limits!(SecretServiceServer::new(SecretSvc::new(
            state.clone()
        )))))
        .add_service(auth.layer(limits!(ExecutionServer::new(exec.clone()))))
        .add_service(auth.layer(limits!(ActionCacheServer::new(exec.clone()))))
        .add_service(auth.layer(limits!(ContentAddressableStorageServer::new(exec.clone()))))
        .add_service(auth.layer(limits!(CapabilitiesServer::new(exec.clone()))))
        .add_service(auth.layer(limits!(ByteStreamServer::new(exec.clone()))))
        .add_service(auth.layer(limits!(OperationQueueServer::new(exec))))
        .add_service(auth.layer(limits!(AdminServiceServer::new(AdminSvc::new(state)))))
        .serve_with_incoming_shutdown(grpc_incoming(listener), shutdown.cancelled_owned())
        .await
        .map_err(|e| Error::Transport(format!("gRPC server: {e}")))
}

/// Connections on the gRPC listener, sent without Nagle's delay: a
/// response spanning several segments otherwise waits on the client's
/// delayed ACK, 40 ms on Linux.
fn grpc_incoming(listener: TcpListener) -> TcpIncoming {
    TcpIncoming::from(listener).with_nodelay(Some(true))
}

async fn serve_http(
    state: Arc<AppState>,
    listener: TcpListener,
    shutdown: CancellationToken,
) -> Result<()> {
    let router = objects::router(state.clone(), state.config.roles.api);
    let Some(tls) = &state.config.tls else {
        use axum::serve::ListenerExt;
        let listener = listener.tap_io(|tcp| {
            if let Err(e) = tcp.set_nodelay(true) {
                tracing::debug!(error = %e, "setting TCP_NODELAY failed");
            }
        });
        return axum::serve(listener, router)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
            .map_err(|e| Error::Transport(format!("HTTP server: {e}")));
    };
    let config = crate::tls::http(tls)?;
    let handle = axum_server::Handle::new();
    let stopping = handle.clone();
    tokio::spawn(async move {
        shutdown.cancelled().await;
        stopping.graceful_shutdown(None);
    });
    let listener = listener
        .into_std()
        .map_err(|e| Error::Transport(format!("HTTP listener: {e}")))?;
    axum_server::from_tcp_rustls(listener, config)
        .map_err(|e| Error::Transport(format!("HTTP listener: {e}")))?
        .handle(handle)
        .serve(router.into_make_service())
        .await
        .map_err(|e| Error::Transport(format!("HTTP server: {e}")))
}

/// Report every service serving, or not serving once the process drains.
async fn set_health(reporter: &tonic_health::server::HealthReporter, serving: bool) {
    let status = if serving {
        tonic_health::ServingStatus::Serving
    } else {
        tonic_health::ServingStatus::NotServing
    };
    for name in [
        "",
        <RepoServiceServer<RepoSvc> as tonic::server::NamedService>::NAME,
        <WorkspaceServiceServer<WorkspaceSvc> as tonic::server::NamedService>::NAME,
        <ReviewServiceServer<ReviewSvc> as tonic::server::NamedService>::NAME,
        <IntentServiceServer<IntentSvc> as tonic::server::NamedService>::NAME,
        <LanderServiceServer<LanderSvc> as tonic::server::NamedService>::NAME,
        <AdminServiceServer<AdminSvc> as tonic::server::NamedService>::NAME,
        <AccessServiceServer<AccessSvc> as tonic::server::NamedService>::NAME,
        <AuditServiceServer<AuditSvc> as tonic::server::NamedService>::NAME,
        <ProtectionServiceServer<ProtectionSvc> as tonic::server::NamedService>::NAME,
        <SecretServiceServer<SecretSvc> as tonic::server::NamedService>::NAME,
        <IdentityServiceServer<IdentitySvc> as tonic::server::NamedService>::NAME,
        <SessionServiceServer<SessionSvc> as tonic::server::NamedService>::NAME,
    ] {
        reporter.set_service_status(name, status).await;
    }
}

#[cfg(test)]
mod tests {
    use tokio_stream::StreamExt;

    use super::*;

    #[tokio::test]
    async fn grpc_connections_disable_nagle() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut incoming = grpc_incoming(listener);
        let _client = tokio::net::TcpStream::connect(addr).await.unwrap();
        let accepted = incoming.next().await.unwrap().unwrap();
        assert!(accepted.nodelay().unwrap());
    }
}
