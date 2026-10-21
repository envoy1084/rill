//! Searches of roots the index has not reached yet.

use super::*;

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
    let mut c = clients(&server, None).await;
    let created = c
        .repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: "r".into(),
            description: None,
            windows_safe: false,
        })
        .await
        .unwrap()
        .into_inner();
    let id: [u8; 16] = created.repo.unwrap().id.as_ref().try_into().unwrap();
    let queue = rill_jobs::Queue::new(
        stores.repo(rill_types::RepoId::from_bytes(id)),
        rill_lander::land::followups::INDEX_QUEUE,
    );
    let version = c
        .repo
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Bytes::new(),
            repo: "r".into(),
            base: None,
            edits: vec![set_edit("a.txt", b"needle\n")],
            message: "a".into(),
            intent: None,
            provenance: None,
            change_id: None,
        })
        .await
        .unwrap()
        .into_inner();

    let wave = || {
        let searches = (0..20).map(|_| {
            let mut repo = c.repo.clone();
            let mut request = Request::new(pb::GrepRequest {
                repo: "r".into(),
                root: Some(pb::RootSpec {
                    root: Some(pb::root_spec::Root::VersionHash(version.hash.clone())),
                }),
                pattern: "needle".into(),
                case_insensitive: false,
                path_globs: vec![],
                max_results: 0,
                context_lines: 0,
            });
            request.set_timeout(Duration::from_millis(500));
            async move { repo.grep(request).await }
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
