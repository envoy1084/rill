//! Binding listeners and running the gRPC and HTTP servers.

use std::net::SocketAddr;
use std::sync::Arc;

use connectrpc::{ConnectRpcService, Limits};
use connectrpc_health::StaticChecker;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tonic::server::NamedService;
use tower::Layer;

use crate::auth::{AuthInterceptor, AuthLayer, AuthService};
use crate::objects;
use rill_reapi_proto::bytestream::byte_stream_server::ByteStreamServer;
use rill_reapi_proto::reapi::action_cache_server::ActionCacheServer;
use rill_reapi_proto::reapi::capabilities_server::CapabilitiesServer;
use rill_reapi_proto::reapi::content_addressable_storage_server::ContentAddressableStorageServer;
use rill_reapi_proto::reapi::execution_server::ExecutionServer;
use rill_reapi_proto::remoteworker::operation_queue_server::OperationQueueServer;

use crate::services::{
    AccessSvc, AdminSvc, AuditSvc, ChecksSvc, ExecSvc, IdentitySvc, IntentSvc, LanderSvc,
    ProtectionSvc, RepoSvc, ReviewSvc, SecretSvc, SessionSvc, WorkspaceSvc,
};
use rill_runtime::error::{Error, Result};
use rill_runtime::state::AppState;

/// Largest request message: a change version (8 MiB) with room for its
/// envelope; blobs stream in smaller pieces.
pub const MAX_REQUEST_BYTES: usize = 16 << 20;
/// Largest REAPI response message; listings and diffs stay well under it.
pub const MAX_RESPONSE_BYTES: usize = 64 << 20;
/// Requests one HTTP/2 connection may have in flight.
pub const MAX_CONCURRENT_PER_CONNECTION: u32 = 1024;

/// A generated REAPI service with the message size limits.
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
    /// The API: Connect, gRPC and gRPC-web, and REAPI.
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

/// The Rill API services, health and reflection, behind the auth
/// interceptor.
fn connect_service(state: &Arc<AppState>) -> Result<(ConnectRpcService, Arc<StaticChecker>)> {
    let router = connectrpc::Router::new()
        .add_service(Arc::new(SessionSvc::new(state.clone())))
        .add_service(Arc::new(RepoSvc::new(state.clone())))
        .add_service(Arc::new(WorkspaceSvc::new(state.clone())))
        .add_service(Arc::new(ReviewSvc::new(state.clone())))
        .add_service(Arc::new(IntentSvc::new(state.clone())))
        .add_service(Arc::new(LanderSvc::new(state.clone())))
        .add_service(Arc::new(IdentitySvc::new(state.clone())))
        .add_service(Arc::new(AccessSvc::new(state.clone())))
        .add_service(Arc::new(AuditSvc::new(state.clone())))
        .add_service(Arc::new(ProtectionSvc::new(state.clone())))
        .add_service(Arc::new(SecretSvc::new(state.clone())))
        .add_service(Arc::new(ChecksSvc::new(state.clone())))
        .add_service(Arc::new(AdminSvc::new(state.clone())));
    let services: Vec<String> = rill_proto::SERVICES.iter().map(|s| s.to_string()).collect();
    let (router, health) = connectrpc_health::install_static(router, services);
    let reflector = connectrpc_reflection::Reflector::from_descriptor_set_bytes(
        rill_proto::FILE_DESCRIPTOR_SET,
    )
    .map_err(|e| Error::Transport(format!("reflection: {e}")))?;
    let router = connectrpc_reflection::install(router, reflector);
    let service = ConnectRpcService::new(router)
        .with_limits(
            Limits::default()
                .with_max_request_body_size(MAX_REQUEST_BYTES)
                .with_max_message_size(MAX_REQUEST_BYTES),
        )
        .with_interceptor(crate::rpc_codes::RecordCodes)
        .with_interceptor(AuthInterceptor::new(state.clone()));
    Ok((service, health))
}

/// REAPI for Bazel and the execution workers, on tonic.
fn reapi_routes(state: &Arc<AppState>) -> axum::Router {
    let auth = AuthLayer::new(state.clone());
    let exec = ExecSvc::new(state.clone());
    tonic::service::Routes::new(auth.layer(limits!(ExecutionServer::new(exec.clone()))))
        .add_service(auth.layer(limits!(ActionCacheServer::new(exec.clone()))))
        .add_service(auth.layer(limits!(ContentAddressableStorageServer::new(exec.clone()))))
        .add_service(auth.layer(limits!(CapabilitiesServer::new(exec.clone()))))
        .add_service(auth.layer(limits!(ByteStreamServer::new(exec.clone()))))
        .add_service(auth.layer(limits!(OperationQueueServer::new(exec))))
        .into_axum_router()
}

async fn serve_grpc(
    state: Arc<AppState>,
    listener: TcpListener,
    shutdown: CancellationToken,
) -> Result<()> {
    let (connect, health) = connect_service(&state)?;
    let draining = state.lifecycle.draining.clone();
    let reporter = health.clone();
    tokio::spawn(async move {
        draining.cancelled().await;
        reporter.shutdown();
    });
    let router = reapi_routes(&state).fallback_service(connect).layer(
        rill_runtime::telemetry::TelemetryLayer::new(state.metrics.clone()),
    );
    serve_router(router, listener, state.config.tls.as_ref(), shutdown, "API").await
}

/// Serve `router` on `listener` (HTTP/1.1 and cleartext or TLS HTTP/2)
/// until `shutdown`, then drain.
async fn serve_router(
    router: axum::Router,
    listener: TcpListener,
    tls: Option<&rill_runtime::config::TlsConfig>,
    shutdown: CancellationToken,
    what: &'static str,
) -> Result<()> {
    let Some(tls) = tls else {
        use axum::serve::ListenerExt;
        let listener = listener.tap_io(|tcp| {
            // A response spanning several segments otherwise waits on the
            // client's delayed ACK, 40 ms on Linux.
            if let Err(e) = tcp.set_nodelay(true) {
                tracing::debug!(error = %e, "setting TCP_NODELAY failed");
            }
        });
        return axum::serve(listener, router)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
            .map_err(|e| Error::Transport(format!("{what} server: {e}")));
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
        .map_err(|e| Error::Transport(format!("{what} listener: {e}")))?;
    let mut server = axum_server::from_tcp_rustls(listener, config)
        .map_err(|e| Error::Transport(format!("{what} listener: {e}")))?
        .handle(handle);
    server
        .http_builder()
        .http2()
        .max_concurrent_streams(MAX_CONCURRENT_PER_CONNECTION);
    server
        .serve(router.into_make_service())
        .await
        .map_err(|e| Error::Transport(format!("{what} server: {e}")))
}

async fn serve_http(
    state: Arc<AppState>,
    listener: TcpListener,
    shutdown: CancellationToken,
) -> Result<()> {
    let router = objects::router(state.clone(), state.config.roles.api);
    serve_router(
        router,
        listener,
        state.config.tls.as_ref(),
        shutdown,
        "HTTP",
    )
    .await
}
