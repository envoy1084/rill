//! The connected client: one pooled HTTP/2 connection, the generated
//! service clients over it, and the headers every call carries.

use std::sync::Arc;
use std::time::Duration;

use connectrpc::client::{ClientBody, ClientTransport, HttpClient};
use connectrpc::ConnectError;
use futures::future::BoxFuture;
use http::{HeaderValue, Uri};
use rill_proto::rill::v1::{
    AccessServiceClient, AdminServiceClient, AuditServiceClient, ChangeServiceClient,
    ChecksServiceClient, ContentServiceClient, EventServiceClient, IdentityServiceClient,
    IntentServiceClient, LandingServiceClient, OperationServiceClient, OrgServiceClient,
    PolicyServiceClient, ProtectionServiceClient, RepoServiceClient, ReviewServiceClient,
    SecretServiceClient, SessionServiceClient, TrunkServiceClient, WorkspaceServiceClient,
};

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

/// The headers every call carries: `authorization: Bearer <token>`, the
/// token read from the client's token source each time, the relation
/// versions the client has seen, and a request id.
#[derive(Clone)]
pub struct CallHeaders {
    source: Option<Arc<dyn TokenSource>>,
    seen: RelationsSeen,
}

impl std::fmt::Debug for CallHeaders {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallHeaders")
            .field("has_source", &self.source.is_some())
            .finish()
    }
}

impl CallHeaders {
    fn apply(&self, headers: &mut http::HeaderMap) -> std::result::Result<(), ConnectError> {
        if let Some(token) = self.source.as_ref().and_then(|s| s.token()) {
            let value = HeaderValue::try_from(format!("Bearer {token}")).map_err(|_| {
                ConnectError::unauthenticated("token contains non-ASCII characters")
            })?;
            headers.insert(http::header::AUTHORIZATION, value);
        }
        if let Some(value) = self
            .seen
            .header()
            .and_then(|h| HeaderValue::try_from(h).ok())
        {
            headers.insert("x-rill-relations", value);
        }
        // An id per request, so a failure here and the server's log lines
        // for it can be matched.
        if !headers.contains_key("x-request-id") {
            let id = hex::encode(&rill_types::ids::random_id()[..8]);
            if let Ok(value) = HeaderValue::try_from(id) {
                headers.insert("x-request-id", value);
            }
        }
        Ok(())
    }
}

/// The body of a response on the SDK transport.
pub type ResponseBody = <HttpClient as ClientTransport>::ResponseBody;

/// The transport every generated client runs over: the pooled connection,
/// with [`CallHeaders`] set on each request.
#[derive(Clone)]
pub struct Transport {
    http: HttpClient,
    headers: Option<CallHeaders>,
    user_agent: HeaderValue,
}

impl ClientTransport for Transport {
    type ResponseBody = ResponseBody;
    type Error = ConnectError;

    fn send(
        &self,
        mut request: http::Request<ClientBody>,
    ) -> BoxFuture<'static, std::result::Result<http::Response<Self::ResponseBody>, ConnectError>>
    {
        if let Some(headers) = &self.headers {
            if let Err(e) = headers.apply(request.headers_mut()) {
                return Box::pin(async move { Err(e) });
            }
        }
        request
            .headers_mut()
            .insert(http::header::USER_AGENT, self.user_agent.clone());
        self.http.send(request)
    }
}

/// A connection to a server without credentials, for signing in.
#[derive(Clone)]
pub struct Connection {
    transport: Transport,
    config: connectrpc::client::ClientConfig,
}

impl Connection {
    /// The session service, which needs no token.
    pub fn sessions(&self) -> SessionServiceClient<Transport> {
        SessionServiceClient::new(self.transport.clone(), self.config.clone())
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
pub(crate) async fn retry_idempotent<T, F, Fut>(call: F) -> std::result::Result<T, ConnectError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = std::result::Result<T, ConnectError>>,
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
        .when(|e: &ConnectError| e.code == connectrpc::ErrorCode::Unavailable)
        .notify(|e, _| tracing::debug!(error = %e, "retrying an idempotent call"))
        .await
}

/// Generated repo client over the SDK transport.
pub type RepoSvc = RepoServiceClient<Transport>;
/// Generated content client over the SDK transport.
pub type ContentSvc = ContentServiceClient<Transport>;
/// Generated change client over the SDK transport.
pub type ChangeSvc = ChangeServiceClient<Transport>;
/// Generated event client over the SDK transport.
pub type EventSvc = EventServiceClient<Transport>;
/// Generated workspace client over the SDK transport.
pub type WorkspaceSvc = WorkspaceServiceClient<Transport>;
/// Generated review client over the SDK transport.
pub type ReviewSvc = ReviewServiceClient<Transport>;
/// Generated intent client over the SDK transport.
pub type IntentSvc = IntentServiceClient<Transport>;
/// Generated landing client over the SDK transport.
pub type LandingSvc = LandingServiceClient<Transport>;
/// Generated trunk client over the SDK transport.
pub type TrunkSvc = TrunkServiceClient<Transport>;
/// Generated operation client over the SDK transport.
pub type OperationSvc = OperationServiceClient<Transport>;
/// Generated policy client over the SDK transport.
pub type PolicySvc = PolicyServiceClient<Transport>;
/// Generated org client over the SDK transport.
pub type OrgSvc = OrgServiceClient<Transport>;
/// Generated admin client over the SDK transport.
pub type AdminSvc = AdminServiceClient<Transport>;
/// Generated audit client over the SDK transport.
pub type AuditSvc = AuditServiceClient<Transport>;
/// Generated protection client over the SDK transport.
pub type ProtectionSvc = ProtectionServiceClient<Transport>;
/// Generated secret client over the SDK transport.
pub type SecretSvc = SecretServiceClient<Transport>;
/// Generated checks client over the SDK transport.
pub type ChecksSvc = ChecksServiceClient<Transport>;
/// Generated access client over the SDK transport.
pub type AccessSvc = AccessServiceClient<Transport>;
/// Generated identity client over the SDK transport.
pub type IdentitySvc = IdentityServiceClient<Transport>;

/// A connection to one Rill server.
///
/// Cloning is cheap: every clone shares the underlying connection pool.
#[derive(Clone)]
pub struct Client {
    config: ClientConfig,
    connection: Connection,
    headers: CallHeaders,
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
    /// Connect to the configured endpoint. A workload credential is
    /// exchanged for a session here; otherwise the connection is opened by
    /// the first request.
    pub async fn connect(config: ClientConfig) -> Result<Self> {
        let connection = Self::connection(&config)?;
        if config.token.is_none() && config.session.is_none() {
            if let Some(workload) = config.workload.clone() {
                let source = WorkloadSource::start(workload, connection.clone()).await?;
                return Ok(Client {
                    config,
                    connection,
                    headers: CallHeaders {
                        source: Some(source),
                        seen: RelationsSeen::default(),
                    },
                    grants: Default::default(),
                });
            }
        }
        Self::with_connection(config, connection)
    }

    /// Like [`Client::connect`], for configurations without a workload
    /// credential, without awaiting anything.
    pub fn connect_lazy(config: ClientConfig) -> Result<Self> {
        let connection = Self::connection(&config)?;
        Self::with_connection(config, connection)
    }

    /// Build a client over an existing connection. A stored session starts
    /// refreshing itself here; an explicit token wins over it.
    pub fn with_connection(config: ClientConfig, connection: Connection) -> Result<Self> {
        let source: Option<Arc<dyn TokenSource>> = match (&config.token, &config.session) {
            (Some(token), _) => {
                if !token.is_ascii() {
                    return Err(Error::config("token contains non-ASCII characters"));
                }
                Some(Arc::new(StaticToken(token.clone())))
            }
            (None, Some(session)) => Some(SessionSource::start(
                session.credentials.clone(),
                connection.clone(),
                session.store.clone(),
            )),
            (None, None) if config.workload.is_some() => {
                return Err(Error::config(
                    "a workload credential is exchanged when connecting; use Client::connect",
                ))
            }
            (None, None) => None,
        };
        let headers = CallHeaders {
            source,
            seen: RelationsSeen::default(),
        };
        Ok(Client {
            config,
            connection,
            headers,
            grants: Default::default(),
        })
    }

    /// Connect using `RILL_ENDPOINT` and `RILL_TOKEN`.
    pub async fn from_env() -> Result<Self> {
        Self::connect(ClientConfig::from_env()?).await
    }

    /// The pooled connection `config` describes, without credentials.
    pub fn connection(config: &ClientConfig) -> Result<Connection> {
        let uri: Uri = config
            .endpoint
            .parse()
            .map_err(|e| Error::config(format!("invalid endpoint {:?}: {e}", config.endpoint)))?;
        let http = match uri.scheme_str() {
            Some("https") => HttpClient::with_tls(Arc::new(tls_config(&config.tls)?)),
            Some("http") => HttpClient::plaintext_http2_only(),
            _ => {
                return Err(Error::config(format!(
                    "endpoint {:?} must start with http:// or https://",
                    config.endpoint
                )))
            }
        };
        let user_agent = HeaderValue::try_from(config.user_agent.clone())
            .map_err(|e| Error::config(format!("invalid user agent: {e}")))?;
        let mut call = connectrpc::client::ClientConfig::new(uri);
        if let Some(t) = config.timeout {
            call = call.with_default_timeout(t);
        }
        Ok(Connection {
            transport: Transport {
                http,
                headers: None,
                user_agent,
            },
            config: call,
        })
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
        &self.headers.seen
    }

    /// The connection, without this client's credentials (signing in).
    pub fn connection_without_credentials(&self) -> Connection {
        self.connection.clone()
    }

    /// The transport carrying this client's credentials, for services the
    /// SDK does not wrap.
    pub fn transport(&self) -> Transport {
        Transport {
            headers: Some(self.headers.clone()),
            ..self.connection.transport.clone()
        }
    }

    /// The per-call configuration (endpoint, protocol, timeout).
    pub fn call_config(&self) -> connectrpc::client::ClientConfig {
        self.connection.config.clone()
    }

    /// A fresh generated repo client.
    pub fn repo_svc(&self) -> RepoSvc {
        RepoServiceClient::new(self.transport(), self.call_config())
    }

    /// A fresh generated content client.
    pub fn content_svc(&self) -> ContentSvc {
        ContentServiceClient::new(self.transport(), self.call_config())
    }

    /// A fresh generated change client.
    pub fn change_svc(&self) -> ChangeSvc {
        ChangeServiceClient::new(self.transport(), self.call_config())
    }

    /// A fresh generated event client.
    pub fn event_svc(&self) -> EventSvc {
        EventServiceClient::new(self.transport(), self.call_config())
    }

    /// A fresh generated workspace client.
    pub fn workspace_svc(&self) -> WorkspaceSvc {
        WorkspaceServiceClient::new(self.transport(), self.call_config())
    }

    /// A fresh generated review client.
    pub fn review_svc(&self) -> ReviewSvc {
        ReviewServiceClient::new(self.transport(), self.call_config())
    }

    /// A fresh generated intent client.
    pub fn intent_svc(&self) -> IntentSvc {
        IntentServiceClient::new(self.transport(), self.call_config())
    }

    /// A fresh generated landing client.
    pub fn landing_svc(&self) -> LandingSvc {
        LandingServiceClient::new(self.transport(), self.call_config())
    }

    /// A fresh generated trunk client.
    pub fn trunk_svc(&self) -> TrunkSvc {
        TrunkServiceClient::new(self.transport(), self.call_config())
    }

    /// A fresh generated operation client.
    pub fn operation_svc(&self) -> OperationSvc {
        OperationServiceClient::new(self.transport(), self.call_config())
    }

    /// A fresh generated policy client.
    pub fn policy_svc(&self) -> PolicySvc {
        PolicyServiceClient::new(self.transport(), self.call_config())
    }

    /// A fresh generated org client.
    pub fn org_svc(&self) -> OrgSvc {
        OrgServiceClient::new(self.transport(), self.call_config())
    }

    /// A fresh generated admin client.
    pub fn admin_svc(&self) -> AdminSvc {
        AdminServiceClient::new(self.transport(), self.call_config())
    }

    /// A fresh generated audit client.
    pub fn audit_svc(&self) -> AuditSvc {
        AuditServiceClient::new(self.transport(), self.call_config())
    }

    /// A fresh generated protection client.
    pub fn protection_svc(&self) -> ProtectionSvc {
        ProtectionServiceClient::new(self.transport(), self.call_config())
    }

    /// A fresh generated checks client.
    pub fn checks_svc(&self) -> ChecksSvc {
        ChecksServiceClient::new(self.transport(), self.call_config())
    }

    /// A fresh generated secret client.
    pub fn secret_svc(&self) -> SecretSvc {
        SecretServiceClient::new(self.transport(), self.call_config())
    }

    /// A fresh generated access client.
    pub fn access_svc(&self) -> AccessSvc {
        AccessServiceClient::new(self.transport(), self.call_config())
    }

    /// A fresh generated identity client.
    pub fn identity_svc(&self) -> IdentitySvc {
        IdentityServiceClient::new(self.transport(), self.call_config())
    }
}

/// TLS for an `https://` endpoint: the system's roots plus any configured
/// CA, and a client certificate when one is configured.
fn tls_config(tls: &crate::config::TlsOptions) -> Result<rustls::ClientConfig> {
    use rustls_pki_types::pem::PemObject;
    use rustls_pki_types::{CertificateDer, PrivateKeyDer};
    let pem_err = |path: &std::path::Path, e: rustls_pki_types::pem::Error| {
        Error::config(format!("tls: {}: {e}", path.display()))
    };
    let mut roots = rustls::RootCertStore::empty();
    for cert in rustls_native_certs::load_native_certs().certs {
        // A certificate the platform store holds but rustls rejects is
        // skipped, as every other client does.
        let _ = roots.add(cert);
    }
    if let Some(ca) = &tls.ca_file {
        for cert in CertificateDer::pem_file_iter(ca).map_err(|e| pem_err(ca, e))? {
            roots
                .add(cert.map_err(|e| pem_err(ca, e))?)
                .map_err(|e| Error::config(format!("tls: {}: {e}", ca.display())))?;
        }
    }
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let builder = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| Error::config(format!("tls: {e}")))?
        .with_root_certificates(roots);
    let mut config = match (&tls.client_cert, &tls.client_key) {
        (Some(cert), Some(key)) => {
            let certs = CertificateDer::pem_file_iter(cert)
                .map_err(|e| pem_err(cert, e))?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| pem_err(cert, e))?;
            let key = PrivateKeyDer::from_pem_file(key).map_err(|e| pem_err(key, e))?;
            builder
                .with_client_auth_cert(certs, key)
                .map_err(|e| Error::config(format!("tls: {e}")))?
        }
        (None, None) => builder.with_no_client_auth(),
        _ => {
            return Err(Error::config(
                "a client certificate needs both RILL_CLIENT_CERT and RILL_CLIENT_KEY",
            ))
        }
    };
    config.alpn_protocols = vec![b"h2".to_vec()];
    Ok(config)
}
