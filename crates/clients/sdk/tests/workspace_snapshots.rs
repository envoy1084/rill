//! Workspace snapshots built server-side from edits: incremental, from the
//! base, and conditional on the etag.

use rill_sdk::testing::DevServer;
use rill_sdk::{Edit, ErrorReason, Root};
use rill_types::{RepoPathBuf, SparseProfile};

fn path(p: &str) -> RepoPathBuf {
    RepoPathBuf::new(p).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn edits_build_the_snapshot_and_a_stale_etag_is_refused() {
    let server = DevServer::start().await.unwrap();
    let client = server.client().await.unwrap();
    client.create_repo("snap", None).await.unwrap();
    let repo = client.repo("snap");
    let workspaces = repo.workspaces();
    let ws = workspaces
        .create("w", None, SparseProfile::default())
        .await
        .unwrap()
        .workspace;
    let names = |hash| {
        let repo = repo.clone();
        async move {
            repo.list_dir(&Root::Tree(hash), &RepoPathBuf::root())
                .await
                .unwrap()
                .into_iter()
                .map(|(n, _)| n)
                .collect::<Vec<_>>()
        }
    };
    let snapshot = |ws| {
        let workspaces = workspaces.clone();
        async move { workspaces.get(ws).await.unwrap().snapshot.unwrap() }
    };

    let first = workspaces
        .snapshot_edits(
            ws.id,
            vec![Edit::set(path("a"), "a"), Edit::set(path("b"), "b")],
            false,
            None,
        )
        .await
        .unwrap();
    let etag = first.id.to_string();
    workspaces
        .snapshot_edits(ws.id, vec![Edit::delete(path("a"))], false, Some(&etag))
        .await
        .unwrap();
    assert_eq!(names(snapshot(ws.id).await).await, ["b"]);

    let stale = workspaces
        .snapshot_edits(ws.id, vec![Edit::set(path("c"), "c")], false, Some(&etag))
        .await
        .unwrap_err();
    assert!(stale.is(ErrorReason::EtagMismatch), "{stale}");

    workspaces
        .snapshot_edits(ws.id, vec![Edit::set(path("c"), "c")], true, None)
        .await
        .unwrap();
    assert_eq!(names(snapshot(ws.id).await).await, ["c"]);

    let patch = Edit::Patch {
        path: path("c"),
        unified: String::new(),
    };
    let refused = workspaces
        .snapshot_edits(ws.id, vec![patch], false, None)
        .await
        .unwrap_err();
    assert_eq!(
        refused.code(),
        Some(rill_sdk::Code::InvalidArgument),
        "{refused}"
    );
}
