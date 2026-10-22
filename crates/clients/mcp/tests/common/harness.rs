//! A mock server on a local port with an MCP server connected to it.

use std::sync::Arc;

use rill_mcp::{McpServer, ToolContext};
use rill_proto::rill::v1::repo_service_server::RepoServiceServer;
use rill_sdk::{Client, ClientConfig};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;

use super::mock_repo::MockRepo;

/// A running mock and an MCP server connected to it.
pub struct Harness {
    pub repo: Arc<MockRepo>,
    pub server: McpServer,
}

impl Harness {
    pub async fn start(repo: MockRepo) -> Self {
        let repo = Arc::new(repo);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let svc = repo.clone();
        tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(RepoServiceServer::from_arc(svc))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .unwrap();
        });
        let client = Client::connect(ClientConfig::new(format!("http://{addr}")))
            .await
            .unwrap();
        let server = McpServer::new(ToolContext::new(client, Some("main".into())));
        Harness { repo, server }
    }
}
