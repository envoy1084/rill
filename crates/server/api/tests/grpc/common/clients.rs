//! Authenticated clients for every service.

use connectrpc::client::{ClientConfig, HttpClient};
use rill_proto::rill::v1::{
    AccessServiceClient, AdminServiceClient, AttentionServiceClient, AuditServiceClient,
    ChangeServiceClient, ChecksServiceClient, ContentServiceClient, EventServiceClient,
    IdentityServiceClient, IntentServiceClient, LandingServiceClient, OperationServiceClient,
    OrgServiceClient, PolicyServiceClient, ProtectionServiceClient, RepoServiceClient,
    ReviewServiceClient, ReviewerServiceClient, SecretServiceClient, SessionServiceClient,
    TrunkServiceClient, WorkspaceServiceClient,
};

use super::server::TestServer;

/// The bearer token a client sends, if any.
#[derive(Clone)]
pub(crate) struct Bearer(pub(crate) Option<String>);

/// The transport every test client runs over.
pub(crate) type Svc = HttpClient;

/// A server endpoint to build clients against.
#[derive(Clone)]
pub(crate) struct Conn {
    http: HttpClient,
    uri: http::Uri,
}

/// A connection to `endpoint` (opened by the first call).
pub(crate) async fn connect(endpoint: &str) -> Conn {
    Conn {
        http: HttpClient::plaintext_http2_only(),
        uri: endpoint.parse().expect("test endpoints are valid"),
    }
}

/// A raw gRPC channel to `endpoint`, for calling any RPC by name.
pub(crate) async fn channel(endpoint: &str) -> tonic::transport::Channel {
    tonic::transport::Channel::from_shared(endpoint.to_string())
        .unwrap()
        .connect()
        .await
        .unwrap()
}

/// Sends a bearer token on REAPI (tonic) calls.
#[derive(Clone)]
pub(crate) struct ReapiBearer(pub(crate) Option<String>);

impl tonic::service::Interceptor for ReapiBearer {
    fn call(
        &mut self,
        mut request: tonic::Request<()>,
    ) -> Result<tonic::Request<()>, tonic::Status> {
        if let Some(token) = &self.0 {
            let value = format!("Bearer {token}").parse().unwrap();
            request.metadata_mut().insert("authorization", value);
        }
        Ok(request)
    }
}

/// The transport REAPI test clients run over.
pub(crate) type ReapiSvc =
    tonic::service::interceptor::InterceptedService<tonic::transport::Channel, ReapiBearer>;

/// A generated client that sends `bearer` on every call.
pub(crate) trait Authed: Sized {
    fn authed(conn: Conn, bearer: Bearer) -> Self;
}

macro_rules! authed {
    ($($client:ident),* $(,)?) => {
        $(impl Authed for $client<Svc> {
            fn authed(conn: Conn, bearer: Bearer) -> Self {
                let mut config = ClientConfig::new(conn.uri);
                if let Some(token) = bearer.0 {
                    config = config.with_default_header("authorization", format!("Bearer {token}"));
                }
                $client::new(conn.http, config)
            }
        })*
    };
}

authed!(
    AccessServiceClient,
    AdminServiceClient,
    AttentionServiceClient,
    AuditServiceClient,
    ChangeServiceClient,
    ChecksServiceClient,
    ContentServiceClient,
    EventServiceClient,
    IdentityServiceClient,
    IntentServiceClient,
    LandingServiceClient,
    OperationServiceClient,
    OrgServiceClient,
    PolicyServiceClient,
    ProtectionServiceClient,
    RepoServiceClient,
    ReviewServiceClient,
    ReviewerServiceClient,
    SecretServiceClient,
    SessionServiceClient,
    TrunkServiceClient,
    WorkspaceServiceClient,
);

pub(crate) struct Clients {
    pub(crate) repo: RepoServiceClient<Svc>,
    pub(crate) content: ContentServiceClient<Svc>,
    pub(crate) changes: ChangeServiceClient<Svc>,
    pub(crate) events: EventServiceClient<Svc>,
    pub(crate) landing: LandingServiceClient<Svc>,
    pub(crate) policy: PolicyServiceClient<Svc>,
    pub(crate) orgs: OrgServiceClient<Svc>,
    pub(crate) identity: IdentityServiceClient<Svc>,
    pub(crate) audit: AuditServiceClient<Svc>,
    pub(crate) protection: ProtectionServiceClient<Svc>,
    pub(crate) access: AccessServiceClient<Svc>,
    pub(crate) workspace: WorkspaceServiceClient<Svc>,
    pub(crate) review: ReviewServiceClient<Svc>,
    pub(crate) admin: AdminServiceClient<Svc>,
    pub(crate) secrets: SecretServiceClient<Svc>,
    pub(crate) intents: IntentServiceClient<Svc>,
    pub(crate) reviewers: ReviewerServiceClient<Svc>,
    pub(crate) trunk: TrunkServiceClient<Svc>,
    pub(crate) attention: AttentionServiceClient<Svc>,
}

pub(crate) async fn clients(server: &TestServer, token: Option<&str>) -> Clients {
    clients_at(&server.grpc, token).await
}

pub(crate) async fn clients_at(endpoint: &str, token: Option<&str>) -> Clients {
    let channel = connect(endpoint).await;
    let bearer = Bearer(token.map(str::to_owned));
    Clients {
        repo: RepoServiceClient::authed(channel.clone(), bearer.clone()),
        content: ContentServiceClient::authed(channel.clone(), bearer.clone()),
        changes: ChangeServiceClient::authed(channel.clone(), bearer.clone()),
        events: EventServiceClient::authed(channel.clone(), bearer.clone()),
        landing: LandingServiceClient::authed(channel.clone(), bearer.clone()),
        policy: PolicyServiceClient::authed(channel.clone(), bearer.clone()),
        orgs: OrgServiceClient::authed(channel.clone(), bearer.clone()),
        identity: IdentityServiceClient::authed(channel.clone(), bearer.clone()),
        access: AccessServiceClient::authed(channel.clone(), bearer.clone()),
        audit: AuditServiceClient::authed(channel.clone(), bearer.clone()),
        protection: ProtectionServiceClient::authed(channel.clone(), bearer.clone()),
        workspace: WorkspaceServiceClient::authed(channel.clone(), bearer.clone()),
        review: ReviewServiceClient::authed(channel.clone(), bearer.clone()),
        admin: AdminServiceClient::authed(channel.clone(), bearer.clone()),
        secrets: SecretServiceClient::authed(channel.clone(), bearer.clone()),
        intents: IntentServiceClient::authed(channel.clone(), bearer.clone()),
        reviewers: ReviewerServiceClient::authed(channel.clone(), bearer.clone()),
        trunk: TrunkServiceClient::authed(channel.clone(), bearer.clone()),
        attention: AttentionServiceClient::authed(channel, bearer),
    }
}

/// A server stream's messages, owned. A failure ends it.
pub(crate) trait Messages<W> {
    fn messages(self) -> futures::stream::BoxStream<'static, Result<W, connectrpc::ConnectError>>;
}

type Body = <HttpClient as connectrpc::client::ClientTransport>::ResponseBody;

impl<V, W> Messages<W> for connectrpc::client::ServerStream<Body, V>
where
    V: buffa::MessageView<'static, Owned = W> + Send + 'static,
    W: buffa::view::HasMessageView<View<'static> = V>
        + buffa::Message
        + connectrpc::JsonDeserialize
        + Send
        + 'static,
{
    fn messages(self) -> futures::stream::BoxStream<'static, Result<W, connectrpc::ConnectError>> {
        use futures::StreamExt;
        futures::stream::unfold(Some(self), |state| async move {
            let mut stream = state?;
            match stream.message::<W>().await {
                Ok(Some(msg)) => Some((Ok(msg.to_owned_message()), Some(stream))),
                Ok(None) => None,
                Err(e) => Some((Err(e), None)),
            }
        })
        .boxed()
    }
}
