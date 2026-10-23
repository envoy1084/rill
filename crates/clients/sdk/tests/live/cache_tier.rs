//! Blob reads through a regional cache fall back to the origin when the
//! cache cannot serve them.

use rill_sdk::testing::DevServer;
use rill_sdk::{ApplyOptions, Client, Edit, EditBase, Root};
use rill_types::{EntryKind, RepoPathBuf};

#[tokio::test(flavor = "multi_thread")]
async fn an_unreachable_cache_falls_back_to_the_origin() {
    let server = DevServer::start().await.unwrap();
    let client = server.client().await.unwrap();
    client.create_repo("tier", None).await.unwrap();
    let big = "blob ".repeat(200);
    let path = RepoPathBuf::new("big.txt").unwrap();
    let version = client
        .repo("tier")
        .apply_edits(
            EditBase::Root(Root::trunk()),
            vec![Edit::set(path.clone(), big.clone())],
            ApplyOptions::message("big"),
        )
        .await
        .unwrap()
        .hash;
    let cached =
        Client::connect_lazy(client.config().clone().with_cache("http://127.0.0.1:9")).unwrap();
    let repo = cached.repo("tier");
    let Some(EntryKind::File { content, .. }) = repo
        .resolve_path(&Root::Version(version), &path)
        .await
        .unwrap()
    else {
        panic!("big.txt is a file");
    };
    assert!(!content.is_inline());
    assert_eq!(&repo.get_blob(&content).await.unwrap()[..], big.as_bytes());
}
