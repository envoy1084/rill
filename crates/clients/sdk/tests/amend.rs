//! Amending a change by naming it as the base keeps its earlier edits.

use rill_sdk::testing::DevServer;
use rill_sdk::{ApplyOptions, Edit, EditBase, Root};
use rill_types::RepoPathBuf;

fn path(p: &str) -> RepoPathBuf {
    RepoPathBuf::new(p).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_amend_on_a_change_base_keeps_the_changes_earlier_files() {
    let server = DevServer::start().await.unwrap();
    let client = server.client().await.unwrap();
    client.create_repo("amend", None).await.unwrap();
    let repo = client.repo("amend");
    let first = repo
        .apply_edits(
            EditBase::Root(Root::trunk()),
            vec![Edit::set(path("a.txt"), "a\n")],
            ApplyOptions::message("a"),
        )
        .await
        .unwrap();
    let id = first.version.change_id;
    let mut opts = ApplyOptions::message("a and b");
    opts.change_id = Some(id);
    let amended = repo
        .apply_edits(
            EditBase::Change(id),
            vec![Edit::set(path("b.txt"), "b\n")],
            opts,
        )
        .await
        .unwrap();
    assert_eq!(amended.version.change_id, id);
    let files: Vec<&str> = amended
        .version
        .file_changes
        .keys()
        .map(|p| p.as_str())
        .collect();
    assert_eq!(files, ["a.txt", "b.txt"]);
    let root = Root::Version(amended.hash);
    assert_eq!(
        &repo.read_file(&root, &path("a.txt")).await.unwrap()[..],
        b"a\n"
    );
}
