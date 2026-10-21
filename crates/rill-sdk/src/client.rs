//! The connected client: one channel, six generated service clients, and
//! a bearer-token interceptor.

use std::time::Duration;

use rill_proto::rill::v1::admin_service_client::AdminServiceClient;
use rill_proto::rill::v1::intent_service_client::IntentServiceClient;
use rill_proto::rill::v1::lander_service_client::LanderServiceClient;
use rill_proto::rill::v1::repo_service_client::RepoServiceClient;
use rill_proto::rill::v1::review_service_client::ReviewServiceClient;
use rill_proto::rill::v1::workspace_service_client::WorkspaceServiceClient;
use tonic::metadata::MetadataValue;
use tonic::service::interceptor::InterceptedService;
use tonic::service::Interceptor;
use tonic::transport::{Channel, Endpoint};
use tonic::{Request, Status};

use crate::admin::AdminClient;
use crate::config::ClientConfig;
use crate::error::{Error, Result};
use crate::repo::RepoClient;

/// Attaches `authorization: Bearer <token>` to every outgoing request.
#[derive(Clone, Debug)]
pub struct AuthInterceptor {
    header: Option<MetadataValue<tonic::metadata::Ascii>>,
}

impl AuthInterceptor {
    fn new(token: Option<&str>) -> Result<Self> {
        let header = match token {
            None => None,
            Some(t) => Some(
                format!("Bearer {t}")
                    .parse()
                    .map_err(|_| Error::config("token contains non-ASCII characters"))?,
            ),
        };
        Ok(AuthInterceptor { header })
    }
}

impl Interceptor for AuthInterceptor {
    fn call(&mut self, mut request: Request<()>) -> std::result::Result<Request<()>, Status> {
        if let Some(h) = &self.header {
            request.metadata_mut().insert("authorization", h.clone());
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

/// A connection to one Rill server.
///
/// Cloning is cheap: every clone shares the underlying HTTP/2 channel.
#[derive(Clone)]
pub struct Client {
    config: ClientConfig,
    channel: Channel,
    auth: AuthInterceptor,
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
    pub fn with_channel(config: ClientConfig, channel: Channel) -> Result<Self> {
        let auth = AuthInterceptor::new(config.token.as_deref())?;
        Ok(Client {
            config,
            channel,
            auth,
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

    /// Identity, tokens, relations and policies.
    pub fn admin(&self) -> AdminClient {
        AdminClient::new(self.clone())
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
