//! Trunk status of a repository nothing has landed in yet.

use rill_sdk::testing::DevServer;

#[tokio::test(flavor = "multi_thread")]
async fn an_empty_trunk_has_status_without_a_head() {
    let server = DevServer::start().await.unwrap();
    let client = server.client().await.unwrap();
    client.create_repo("empty", None).await.unwrap();
    let status = client.repo("empty").trunk().trunk_status(5).await.unwrap();
    assert_eq!(status.head_seq.get(), 0);
    assert_eq!(status.head_hash, None);
    assert!(status.recent.is_empty());
}
