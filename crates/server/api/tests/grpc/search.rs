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

async fn grep(
    c: &Clients,
    revision: &str,
    pattern: &str,
    min_seq: Option<u64>,
) -> pb::GrepResponse {
    c.content
        .grep(pb::GrepRequest {
            repo: common::names::repo("r"),
            revision: revision.into(),
            pattern: pattern.into(),
            min_seq,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
}

/// A search can wait for the landing it just made, and a change that has
/// not landed is searched as its indexed parent plus its own files.
#[tokio::test(flavor = "multi_thread")]
async fn searches_wait_for_landings_and_cover_unlanded_changes() {
    let server = TestServer::start(dev_config()).await;
    let mut c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            parent: common::names::org("default"),
            repo_id: "r".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let landed = c
        .changes
        .apply_edits(pb::ApplyEditsRequest {
            parent: common::names::repo("r"),
            edits: vec![
                set_edit("src/lib.rs", b"pub fn landed_thing() { a.b(1) }\n"),
                set_edit("docs/guide.md", b"landed_thing explained\n"),
            ],
            message: "first".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .info
        .unwrap();
    let seq = land(&mut c, "r", landed.change_id.clone()).await;
    let r = grep(&c, "trunk", "landed_thing", Some(seq)).await;
    assert!(r.indexed_through >= seq);
    assert_eq!(r.files_matched, 2);
    assert_eq!(r.languages.get("Rust"), Some(&1));
    assert_eq!(r.directories.get("docs"), Some(&1));
    let literal = c
        .content
        .grep(pb::GrepRequest {
            repo: common::names::repo("r"),
            pattern: "a.b(".into(),
            literal: true,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned();
    assert_eq!(literal.hits.len(), 1);

    let pending = c
        .changes
        .apply_edits(pb::ApplyEditsRequest {
            parent: common::names::repo("r"),
            base: Some(common::trunk_base()).into(),
            edits: vec![set_edit("src/new.rs", b"fn only_in_the_change() {}\n")],
            message: "pending".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .info
        .unwrap();
    let revision = format!("changes/{}", hex::encode(&pending.change_id));
    let r = grep(&c, &revision, "only_in_the_change|landed_thing", None).await;
    let paths: Vec<_> = r.hits.iter().map(|h| h.path.as_str()).collect();
    assert!(
        paths.contains(&"src/new.rs") && paths.contains(&"src/lib.rs"),
        "{paths:?}"
    );
    assert!(grep(&c, "trunk", "only_in_the_change", None)
        .await
        .hits
        .is_empty());
    server.stop().await;
}
