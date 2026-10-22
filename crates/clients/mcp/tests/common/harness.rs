//! A mock server on a local port with an MCP server connected to it.

use std::sync::Arc;

use rill_mcp::{McpServer, ToolContext};
use rill_sdk::{Client, ClientConfig};
use tokio::net::TcpListener;

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
        let router = connectrpc::Router::new();
        let router = rill_proto::rill::v1::RepoServiceExt::register(repo.clone(), router);
        let router = rill_proto::rill::v1::ContentServiceExt::register(repo.clone(), router);
        tokio::spawn(async move {
            axum::serve(
                listener,
                axum::Router::new().fallback_service(router.into_axum_service()),
            )
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
