//! A mock server on a local port with a connected SDK client.

use std::sync::Arc;

use rill_proto::rill::v1::repo_service_server::RepoServiceServer;
use rill_proto::rill::v1::review_service_server::ReviewServiceServer;
use rill_sdk::{Client, ClientConfig};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;

use super::mock_repo::MockRepo;
use super::mock_review::MockReview;

/// A running mock server and a client connected to it.
pub struct Harness {
    pub client: Client,
    pub repo: Arc<MockRepo>,
    pub review: Arc<MockReview>,
    pub endpoint: String,
}

impl Harness {
    pub async fn start(repo: MockRepo, review: MockReview, token: Option<&str>) -> Self {
        let repo = Arc::new(repo);
        let review = Arc::new(review);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (repo_s, review_s) = (repo.clone(), review.clone());
        tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(RepoServiceServer::from_arc(repo_s))
                .add_service(ReviewServiceServer::from_arc(review_s))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .unwrap();
        });
        let endpoint = format!("http://{addr}");
        let mut config = ClientConfig::new(endpoint.clone());
        config.token = token.map(str::to_string);
        let client = Client::connect(config).await.unwrap();
        Harness {
            client,
            repo,
            review,
            endpoint,
        }
    }

    pub async fn with_repo(repo: MockRepo) -> Self {
        Self::start(repo, MockReview::default(), None).await
    }
}
