//! The connected client: one channel, six generated service clients, and
//! a bearer-token interceptor.

use std::sync::Arc;
use std::time::Duration;

use rill_proto::rill::v1::access_service_client::AccessServiceClient;
use rill_proto::rill::v1::admin_service_client::AdminServiceClient;
use rill_proto::rill::v1::audit_service_client::AuditServiceClient;
use rill_proto::rill::v1::identity_service_client::IdentityServiceClient;
use rill_proto::rill::v1::intent_service_client::IntentServiceClient;
use rill_proto::rill::v1::lander_service_client::LanderServiceClient;
use rill_proto::rill::v1::protection_service_client::ProtectionServiceClient;
use rill_proto::rill::v1::repo_service_client::RepoServiceClient;
use rill_proto::rill::v1::review_service_client::ReviewServiceClient;
use rill_proto::rill::v1::workspace_service_client::WorkspaceServiceClient;
use tonic::metadata::MetadataValue;
use tonic::service::interceptor::InterceptedService;
use tonic::service::Interceptor;
use tonic::transport::{Channel, Endpoint};
use tonic::{Request, Status};

use crate::access::AccessClient;
use crate::admin::AdminClient;
use crate::auth::{SessionSource, StaticToken, TokenSource, WorkloadSource};
use crate::config::ClientConfig;
use crate::error::{Error, Result};
use crate::repo::RepoClient;

/// Relation versions this client has observed, by repository id. Every
/// request names them so any server answers with at least those tuples.
#[derive(Clone, Debug, Default)]
pub struct RelationsSeen(Arc<parking_lot::Mutex<std::collections::BTreeMap<String, i64>>>);

impl RelationsSeen {
    /// Remember that `repo_id` reached `version`.
    pub fn observe(&self, repo_id: &str, version: i64) {
        let mut seen = self.0.lock();
        let entry = seen.entry(repo_id.to_string()).or_default();
        *entry = (*entry).max(version);
    }

    fn header(&self) -> Option<String> {
        let seen = self.0.lock();
        (!seen.is_empty()).then(|| {
            seen.iter()
                .map(|(repo, v)| format!("{repo}={v}"))
                .collect::<Vec<_>>()
                .join(",")
        })
    }
}

/// Attaches `authorization: Bearer <token>` to every outgoing request, the
/// token read from the client's token source each time, and the relation
/// versions the client has seen.
#[derive(Clone)]
pub struct AuthInterceptor {
    source: Option<Arc<dyn TokenSource>>,
    seen: RelationsSeen,
}

impl std::fmt::Debug for AuthInterceptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthInterceptor")
            .field("has_source", &self.source.is_some())
            .finish()
    }
}

impl Interceptor for AuthInterceptor {
    fn call(&mut self, mut request: Request<()>) -> std::result::Result<Request<()>, Status> {
        if let Some(token) = self.source.as_ref().and_then(|s| s.token()) {
            let header: MetadataValue<tonic::metadata::Ascii> =
                format!("Bearer {token}")
                    .parse()
                    .map_err(|_| Status::unauthenticated("token contains non-ASCII characters"))?;
            request.metadata_mut().insert("authorization", header);
        }
        if let Some(value) = self.seen.header().and_then(|h| h.parse().ok()) {
            request.metadata_mut().insert("x-rill-relations", value);
        }
        // An id per request, so a failure here and the server's log lines
        // for it can be matched.
        if !request.metadata().contains_key("x-request-id") {
            let id = hex::encode(&rill_types::ids::random_id()[..8]);
            if let Ok(value) = id.parse() {
                request.metadata_mut().insert("x-request-id", value);
            }
        }
        Ok(request)
    }
}

/// How long an idempotent call keeps retrying: past a dead server's
/// idempotency claim (15 s on the server).
const RETRY_BUDGET: Duration = Duration::from_secs(20);

/// Run a read, or a call that carries an operation id, retrying while the
/// cluster is unavailable (a dead or draining server, a cut connection, the
/// claim of a server that died mid-request). Reads have no effect and the
/// server runs an identified operation at most once, so retrying cannot
/// repeat an effect.
pub(crate) async fn retry_idempotent<T, F, Fut>(
    call: F,
) -> std::result::Result<tonic::Response<T>, Status>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = std::result::Result<tonic::Response<T>, Status>>,
{
    use backon::Retryable;
    let schedule = backon::ExponentialBuilder::default()
        .with_min_delay(Duration::from_millis(100))
        .with_max_delay(Duration::from_secs(2))
        .with_jitter()
        .with_total_delay(Some(RETRY_BUDGET))
        .without_max_times();
    call.retry(schedule)
        .sleep(tokio::time::sleep)
        .when(|status: &Status| status.code() == tonic::Code::Unavailable)
        .notify(|status, _| tracing::debug!(error = %status, "retrying an idempotent call"))
        .await
}

/// The transport every generated client runs over.
pub type Svc = InterceptedService<Channel, AuthInterceptor>;

/// Generated repo client over the SDK transport.
pub type RepoSvc = RepoServiceClient<Svc>;
/// Generated workspace client over the SDK transport.
pub type WorkspaceSvc = WorkspaceServiceClient<Svc>;
/// Generated review client over the SDK transport.
pub type ReviewSvc = ReviewServiceClient<Svc>;
/// Generated intent client over the SDK transport.
pub type IntentSvc = IntentServiceClient<Svc>;
/// Generated lander client over the SDK transport.
pub type LanderSvc = LanderServiceClient<Svc>;
/// Generated admin client over the SDK transport.
pub type AdminSvc = AdminServiceClient<Svc>;
/// Generated audit client over the SDK transport.
pub type AuditSvc = AuditServiceClient<Svc>;
/// Generated protection client over the SDK transport.
pub type ProtectionSvc = ProtectionServiceClient<Svc>;
/// Generated access client over the SDK transport.
pub type AccessSvc = AccessServiceClient<Svc>;
/// Generated identity client over the SDK transport.
pub type IdentitySvc = IdentityServiceClient<Svc>;

/// A connection to one Rill server.
///
/// Cloning is cheap: every clone shares the underlying HTTP/2 channel.
#[derive(Clone)]
pub struct Client {
    config: ClientConfig,
    channel: Channel,
    auth: AuthInterceptor,
    grants: crate::grants::ReadGrants,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("endpoint", &self.config.endpoint)
            .field("has_token", &self.config.token.is_some())
            .finish()
    }
}

impl Client {
    /// Connect to the configured endpoint. The connection is established
    /// eagerly so configuration errors surface here rather than on the
    /// first request.
    pub async fn connect(config: ClientConfig) -> Result<Self> {
        let endpoint = Self::endpoint(&config)?;
        let channel = endpoint.connect().await?;
        if config.token.is_none() && config.session.is_none() {
            if let Some(workload) = config.workload.clone() {
                let source = WorkloadSource::start(workload, channel.clone()).await?;
                return Ok(Client {
                    config,
                    channel,
                    auth: AuthInterceptor {
                        source: Some(source),
                        seen: RelationsSeen::default(),
                    },
                    grants: Default::default(),
                });
            }
        }
        Self::with_channel(config, channel)
    }

    /// Like [`Client::connect`] but defers the TCP connection to the first
    /// request.
    pub fn connect_lazy(config: ClientConfig) -> Result<Self> {
        let endpoint = Self::endpoint(&config)?;
        let channel = endpoint.connect_lazy();
        Self::with_channel(config, channel)
    }

    /// Build a client over an existing channel (tests, custom transports).
    /// A stored session starts refreshing itself here; an explicit token
    /// wins over it.
    pub fn with_channel(config: ClientConfig, channel: Channel) -> Result<Self> {
        let source: Option<Arc<dyn TokenSource>> = match (&config.token, &config.session) {
            (Some(token), _) => {
                if !token.is_ascii() {
                    return Err(Error::config("token contains non-ASCII characters"));
                }
                Some(Arc::new(StaticToken(token.clone())))
            }
            (None, Some(session)) => Some(SessionSource::start(
                session.credentials.clone(),
                channel.clone(),
                session.store.clone(),
            )),
            (None, None) if config.workload.is_some() => {
                return Err(Error::config(
                    "a workload credential is exchanged when connecting; use Client::connect",
                ))
            }
            (None, None) => None,
        };
        let auth = AuthInterceptor {
            source,
            seen: RelationsSeen::default(),
        };
        Ok(Client {
            config,
            channel,
            auth,
            grants: Default::default(),
        })
    }

    /// Connect using `RILL_ENDPOINT` and `RILL_TOKEN`.
    pub async fn from_env() -> Result<Self> {
        Self::connect(ClientConfig::from_env()?).await
    }

    fn endpoint(config: &ClientConfig) -> Result<Endpoint> {
        let mut endpoint = Endpoint::from_shared(config.endpoint.clone())
            .map_err(|e| Error::config(format!("invalid endpoint {:?}: {e}", config.endpoint)))?
            .user_agent(config.user_agent.clone())
            .map_err(|e| Error::config(format!("invalid user agent: {e}")))?
            .connect_timeout(Duration::from_secs(10))
            .tcp_keepalive(Some(Duration::from_secs(30)))
            .http2_keep_alive_interval(Duration::from_secs(30));
        if let Some(t) = config.timeout {
            endpoint = endpoint.timeout(t);
        }
        if config.endpoint.starts_with("https://") {
            endpoint = endpoint
                .tls_config(tls_config(&config.tls)?)
                .map_err(|e| Error::config(format!("tls: {e}")))?;
        }
        Ok(endpoint)
    }

    /// The configuration this client was built from.
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Operations on one repository, by name or id.
    pub fn repo(&self, name_or_id: impl Into<String>) -> RepoClient {
        RepoClient::new(self.clone(), name_or_id.into())
    }

    /// Identity, tokens and policies.
    pub fn admin(&self) -> AdminClient {
        AdminClient::new(self.clone())
    }

    /// Org audit logs.
    pub fn audit(&self) -> crate::audit::AuditClient {
        crate::audit::AuditClient::new(self.clone())
    }

    /// Relationship tuples and questions about them.
    pub fn access(&self) -> AccessClient {
        AccessClient::new(self.clone())
    }

    /// Read grants this client has been given; shared by its clones.
    pub fn read_grants(&self) -> &crate::grants::ReadGrants {
        &self.grants
    }

    /// Relation versions this client has seen; shared by its clones.
    pub fn relations_seen(&self) -> &RelationsSeen {
        &self.auth.seen
    }

    /// The underlying channel, for services the SDK does not wrap.
    pub fn channel(&self) -> Channel {
        self.channel.clone()
    }

    fn svc(&self) -> Svc {
        InterceptedService::new(self.channel.clone(), self.auth.clone())
    }

    /// A fresh generated repo client.
    pub fn repo_svc(&self) -> RepoSvc {
        RepoServiceClient::new(self.svc())
    }

    /// A fresh generated workspace client.
    pub fn workspace_svc(&self) -> WorkspaceSvc {
        WorkspaceServiceClient::new(self.svc())
    }

    /// A fresh generated review client.
    pub fn review_svc(&self) -> ReviewSvc {
        ReviewServiceClient::new(self.svc())
    }

    /// A fresh generated intent client.
    pub fn intent_svc(&self) -> IntentSvc {
        IntentServiceClient::new(self.svc())
    }

    /// A fresh generated lander client.
    pub fn lander_svc(&self) -> LanderSvc {
        LanderServiceClient::new(self.svc())
    }

    /// A fresh generated admin client.
    pub fn admin_svc(&self) -> AdminSvc {
        AdminServiceClient::new(self.svc())
    }

    /// A fresh generated audit client.
    pub fn audit_svc(&self) -> AuditSvc {
        AuditServiceClient::new(self.svc())
    }

    /// A fresh generated protection client.
    pub fn protection_svc(&self) -> ProtectionSvc {
        ProtectionServiceClient::new(self.svc())
    }

    /// A fresh generated access client.
    pub fn access_svc(&self) -> AccessSvc {
        AccessServiceClient::new(self.svc())
    }

    /// A fresh generated identity client.
    pub fn identity_svc(&self) -> IdentitySvc {
        IdentityServiceClient::new(self.svc())
    }
}

/// TLS for an `https://` endpoint: the system's roots plus any configured
/// CA, and a client certificate when one is configured.
fn tls_config(tls: &crate::config::TlsOptions) -> Result<tonic::transport::ClientTlsConfig> {
    let read = |path: &std::path::Path| {
        std::fs::read(path)
            .map_err(|e| Error::config(format!("tls: reading {}: {e}", path.display())))
    };
    let mut config = tonic::transport::ClientTlsConfig::new().with_native_roots();
    if let Some(ca) = &tls.ca_file {
        config = config.ca_certificate(tonic::transport::Certificate::from_pem(read(ca)?));
    }
    match (&tls.client_cert, &tls.client_key) {
        (Some(cert), Some(key)) => {
            config = config.identity(tonic::transport::Identity::from_pem(
                read(cert)?,
                read(key)?,
            ));
        }
        (None, None) => {}
        _ => {
            return Err(Error::config(
                "a client certificate needs both RILL_CLIENT_CERT and RILL_CLIENT_KEY",
            ))
        }
    }
    Ok(config)
}
