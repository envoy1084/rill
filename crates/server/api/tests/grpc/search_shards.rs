//! Servers that each serve some search shards answer every search
//! together.

use super::*;
use crate::common::names;

fn serving(shards: std::ops::Range<u16>) -> ServerConfig {
    let mut config = dev_config();
    config.search.serve_shards = shards.collect();
    config
}

async fn search(c: &Clients) -> pb::GrepResponse {
    c.content
        .grep(pb::GrepRequest {
            repo: names::repo("r"),
            pattern: "needle \\d+".into(),
            min_seq: Some(1),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
}

/// Each server registers the shards it serves, answers a peer's shard
/// search with just those shards, and a search on either sees every file;
/// with a peer gone, its shards are searched locally.
#[tokio::test(flavor = "multi_thread")]
async fn shard_servers_answer_searches_together() {
    let stores = rill_store::Stores::memory();
    let a = TestServer::start_on(serving(0..8), stores.clone()).await;
    let b = TestServer::start_on(serving(8..16), stores.clone()).await;
    let (mut ca, cb) = (clients(&a, None).await, clients(&b, None).await);
    let created = ca
        .repo
        .create_repo(pb::CreateRepoRequest {
            parent: names::org("default"),
            repo_id: "r".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned();
    let id: [u8; 16] = created.repo.unwrap().uid.as_ref().try_into().unwrap();
    let files: Vec<_> = (0..40)
        .map(|i| set_edit(&format!("f{i}.txt"), format!("needle {i}\n").as_bytes()))
        .collect();
    let version = ca
        .changes
        .apply_edits(pb::ApplyEditsRequest {
            parent: names::repo("r"),
            edits: files,
            message: "files".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .info
        .unwrap();
    land(&mut ca, "r", version.change_id.clone()).await;

    assert_eq!(search(&ca).await.files_matched, 40);
    assert_eq!(search(&cb).await.files_matched, 40);

    let registry = stores
        .repo(rill_types::RepoId::from_bytes(id))
        .get_misc("search-nodes")
        .await
        .unwrap()
        .expect("serving servers register");
    let registry = String::from_utf8(registry.to_vec()).unwrap();
    assert!(
        registry.contains(&a.grpc) && registry.contains(&b.grpc),
        "{registry}"
    );

    let part = |shards: Vec<u32>| {
        let content = cb.content.clone();
        async move {
            content
                .grep_shards(pb::GrepShardsRequest {
                    grep: Some(pb::GrepRequest {
                        repo: names::repo("r"),
                        revision: "trunk".into(),
                        pattern: "needle".into(),
                        ..Default::default()
                    })
                    .into(),
                    shards,
                })
                .await
                .unwrap()
                .into_owned()
                .files
                .len()
        }
    };
    let (low, high) = (part((0..8).collect()).await, part((8..16).collect()).await);
    assert!(low > 0 && high > 0, "40 files spread over both halves");
    assert_eq!(low + high, 40);

    b.stop().await;
    assert_eq!(search(&ca).await.files_matched, 40);
    a.stop().await;
}
