//! Searches of roots the index has not reached yet.

use super::*;
use connectrpc::client::CallOptions;

/// Concurrent searches of one unindexed root queue one index job, and a
/// cancelled search does not keep later ones from asking again. Every
/// search of a new trunk head used to queue its own job, which grew the
/// index queue without bound under load.
#[tokio::test(flavor = "multi_thread")]
async fn concurrent_searches_of_one_root_share_an_index_job() {
    let stores = rill_store::Stores::memory();
    // No workers role: nothing indexes, so every search keeps waiting.
    let mut config = dev_config();
    config.roles.workers = false;
    let server = TestServer::start_on(config, stores.clone()).await;
    let c = clients(&server, None).await;
    let created = c
        .repo
        .create_repo(pb::CreateRepoRequest {
            parent: common::names::org("default"),
            repo_id: "r".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned();
    let id: [u8; 16] = created.repo.unwrap().uid.as_ref().try_into().unwrap();
    let queue = rill_jobs::Queue::new(
        stores.repo(rill_types::RepoId::from_bytes(id)),
        rill_lander::land::followups::INDEX_QUEUE,
    );
    let version = c
        .changes
        .apply_edits(pb::ApplyEditsRequest {
            parent: common::names::repo("r"),
            edits: vec![set_edit("a.txt", b"needle\n")],
            message: "a".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .info
        .unwrap();

    let wave = || {
        let searches = (0..20).map(|_| {
            let content = c.content.clone();
            let request = pb::GrepRequest {
                repo: common::names::repo("r"),
                revision: hex::encode(&version.hash),
                pattern: "needle".into(),
                ..Default::default()
            };
            let options = CallOptions::default().with_timeout(Duration::from_millis(500));
            async move { content.grep_with_options(request, options).await }
        });
        futures::future::join_all(searches)
    };
    for result in wave().await {
        assert!(result.is_err(), "nothing indexes the root");
    }
    assert_eq!(queue.stats().await.unwrap().enqueued, 1);

    tokio::time::sleep(Duration::from_millis(200)).await;
    wave().await;
    assert_eq!(queue.stats().await.unwrap().enqueued, 2);
    server.stop().await;
}
