//! Authenticated gRPC clients for every service.

use std::time::Duration;

use rill_proto::rill::v1::access_service_client::AccessServiceClient;
use rill_proto::rill::v1::admin_service_client::AdminServiceClient;
use rill_proto::rill::v1::audit_service_client::AuditServiceClient;
use rill_proto::rill::v1::protection_service_client::ProtectionServiceClient;
use rill_proto::rill::v1::repo_service_client::RepoServiceClient;
use rill_proto::rill::v1::review_service_client::ReviewServiceClient;
use rill_proto::rill::v1::secret_service_client::SecretServiceClient;
use rill_proto::rill::v1::workspace_service_client::WorkspaceServiceClient;
use tonic::metadata::MetadataValue;
use tonic::service::interceptor::InterceptedService;
use tonic::service::Interceptor;
use tonic::transport::Channel;
use tonic::{Request, Status};

use super::server::TestServer;

#[derive(Clone)]
pub(crate) struct Bearer(pub(crate) Option<String>);

impl Interceptor for Bearer {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        if let Some(token) = &self.0 {
            let value = MetadataValue::try_from(format!("Bearer {token}")).unwrap();
            request.metadata_mut().insert("authorization", value);
        }
        Ok(request)
    }
}

pub(crate) type Svc = InterceptedService<Channel, Bearer>;

pub(crate) struct Clients {
    pub(crate) repo: RepoServiceClient<Svc>,
    pub(crate) audit: AuditServiceClient<Svc>,
    pub(crate) protection: ProtectionServiceClient<Svc>,
    pub(crate) access: AccessServiceClient<Svc>,
    pub(crate) workspace: WorkspaceServiceClient<Svc>,
    pub(crate) review: ReviewServiceClient<Svc>,
    pub(crate) admin: AdminServiceClient<Svc>,
    pub(crate) secrets: SecretServiceClient<Svc>,
}

pub(crate) async fn clients(server: &TestServer, token: Option<&str>) -> Clients {
    clients_at(&server.grpc, token).await
}

/// A channel to `endpoint`, retried while the server starts.
pub(crate) async fn connect(endpoint: &str) -> Channel {
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

pub(crate) async fn clients_at(endpoint: &str, token: Option<&str>) -> Clients {
    let channel = connect(endpoint).await;
    let bearer = Bearer(token.map(str::to_owned));
    Clients {
        repo: RepoServiceClient::with_interceptor(channel.clone(), bearer.clone()),
        access: AccessServiceClient::with_interceptor(channel.clone(), bearer.clone()),
        audit: AuditServiceClient::with_interceptor(channel.clone(), bearer.clone()),
        protection: ProtectionServiceClient::with_interceptor(channel.clone(), bearer.clone()),
        workspace: WorkspaceServiceClient::with_interceptor(channel.clone(), bearer.clone()),
        review: ReviewServiceClient::with_interceptor(channel.clone(), bearer.clone()),
        admin: AdminServiceClient::with_interceptor(channel.clone(), bearer.clone()),
        secrets: SecretServiceClient::with_interceptor(channel, bearer),
    }
}
