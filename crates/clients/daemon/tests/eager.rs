//! Eager mounts: checkout, status from the overlay, sync to the server
//! workspace, commit, update with merges and conflicts, restart.

mod common;

use common::{path, read, write, Fixture};
use rill_daemon::proto as pb;
use rill_sdk::Root;
use rill_types::RepoPathBuf;

const ADDED: pb::ChangeKind = pb::ChangeKind::Added;
const MODIFIED: pb::ChangeKind = pb::ChangeKind::Modified;
const DELETED: pb::ChangeKind = pb::ChangeKind::Deleted;
const CONFLICT: pb::ChangeKind = pb::ChangeKind::Conflict;

#[tokio::test(flavor = "multi_thread")]
async fn status_reports_edits_and_forgets_reverted_ones() {
    let fx = Fixture::start().await;
    let v1 = fx
        .repo_with(
            "st",
            &[("a.txt", "a\n"), ("c.txt", "c\n"), ("d/e.txt", "e\n")],
        )
        .await;
    let mount = fx.checkout("st", v1, "st").await;
    let dir = fx.checkout_dir("st");
    assert_eq!(read(&dir, "d/e.txt"), "e\n");
    assert!(fx.status(&mount).await.is_empty());

    write(&dir, "a.txt", "changed\n");
    write(&dir, "b.txt", "new\n");
    std::fs::remove_file(dir.join("c.txt")).unwrap();
    std::fs::remove_dir_all(dir.join("d")).unwrap();
    let status = fx.status(&mount).await;
    assert_eq!(
        status,
        vec![
            ("a.txt".into(), MODIFIED),
            ("b.txt".into(), ADDED),
            ("c.txt".into(), DELETED),
            ("d/e.txt".into(), DELETED),
        ]
    );

    write(&dir, "a.txt", "a\n");
    write(&dir, "d/e.txt", "e\n");
    let status = fx.status(&mount).await;
    assert_eq!(
        status,
        vec![("b.txt".into(), ADDED), ("c.txt".into(), DELETED)]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn ignored_files_are_not_recorded_until_tracked() {
    let fx = Fixture::start().await;
    let v1 = fx
        .repo_with("ig", &[(".rillignore", "*.log\n"), ("keep.log", "k\n")])
        .await;
    let mount = fx.checkout("ig", v1, "ig").await;
    let dir = fx.checkout_dir("ig");
    write(&dir, "new.log", "x\n");
    write(&dir, "keep.log", "changed\n");
    assert_eq!(fx.status(&mount).await, vec![("keep.log".into(), MODIFIED)]);
}

#[tokio::test(flavor = "multi_thread")]
async fn edits_reach_the_server_workspace() {
    let fx = Fixture::start().await;
    let v1 = fx.repo_with("sy", &[("a.txt", "a\n")]).await;
    let mount = fx.checkout("sy", v1, "sy").await;
    let dir = fx.checkout_dir("sy");
    write(&dir, "b.txt", &"large ".repeat(200));
    let synced = fx
        .rpc
        .sync_mount(pb::SyncMountRequest {
            name: mount.clone(),
        })
        .await
        .unwrap();
    assert_eq!(synced.mount.unwrap().pending, 0);

    let m = fx.mount(&mount).await;
    let ws = fx
        .repo("sy")
        .workspaces()
        .get(m.workspace.parse().unwrap())
        .await
        .unwrap();
    let names: Vec<String> = fx
        .repo("sy")
        .list_dir(&Root::Tree(ws.snapshot.unwrap()), &RepoPathBuf::root())
        .await
        .unwrap()
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert_eq!(names, ["a.txt", "b.txt"]);

    // Another writer moves the workspace; the next push rebuilds the
    // snapshot from the base instead of failing.
    fx.repo("sy")
        .workspaces()
        .snapshot_edits(
            ws.id,
            vec![rill_sdk::Edit::set(path("z.txt"), "z")],
            false,
            None,
        )
        .await
        .unwrap();
    std::fs::remove_file(dir.join("a.txt")).unwrap();
    fx.rpc
        .sync_mount(pb::SyncMountRequest {
            name: mount.clone(),
        })
        .await
        .unwrap();
    let ws = fx.repo("sy").workspaces().get(ws.id).await.unwrap();
    let names: Vec<String> = fx
        .repo("sy")
        .list_dir(&Root::Tree(ws.snapshot.unwrap()), &RepoPathBuf::root())
        .await
        .unwrap()
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert_eq!(names, ["b.txt"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn commits_amend_and_stack() {
    let fx = Fixture::start().await;
    let v1 = fx.repo_with("cm", &[("a.txt", "a\n")]).await;
    let mount = fx.checkout("cm", v1, "cm").await;
    let dir = fx.checkout_dir("cm");
    let commit = |message: &str, new_change: bool| pb::CommitMountRequest {
        name: mount.clone(),
        message: message.into(),
        new_change,
        ..Default::default()
    };

    let amend_first = fx.rpc.commit_mount(commit("x", false)).await.unwrap_err();
    assert!(
        amend_first.to_string().contains("no working change"),
        "{amend_first}"
    );

    write(&dir, "a.txt", "one\n");
    let first = fx.rpc.commit_mount(commit("first", true)).await.unwrap();
    write(&dir, "b.txt", "b\n");
    let amended = fx
        .rpc
        .commit_mount(commit("first, amended", false))
        .await
        .unwrap();
    assert_eq!(first.change, amended.change);
    let repo = fx.repo("cm");
    let hash = amended.version.parse().unwrap();
    assert_eq!(
        &repo
            .read_file(&Root::Version(hash), &path("b.txt"))
            .await
            .unwrap()[..],
        b"b\n"
    );

    write(&dir, "c.txt", "c\n");
    let second = fx.rpc.commit_mount(commit("second", true)).await.unwrap();
    assert_eq!(second.stacked_on, first.change);
    assert_ne!(second.change, first.change);
    let info = repo
        .change_info(second.version.parse().unwrap())
        .await
        .unwrap();
    let parent = repo.change_version(info.hash).await.unwrap().parents[0];
    assert_eq!(parent, hash, "the new change sits on the first");
    assert_eq!(fx.status(&mount).await, vec![("c.txt".into(), ADDED)]);
}

#[tokio::test(flavor = "multi_thread")]
async fn update_merges_clean_edits_and_flags_conflicts_until_resolved() {
    let fx = Fixture::start().await;
    let v1 = fx
        .repo_with(
            "up",
            &[
                ("x.txt", "1\n2\n3\n4\n5\n"),
                ("y.txt", "y\n"),
                ("z.txt", "z\n"),
            ],
        )
        .await;
    let v2 = fx
        .version(
            "up",
            Root::Version(v1),
            &[
                ("x.txt", "one\n2\n3\n4\n5\n"),
                ("y.txt", "upstream\n"),
                ("w.txt", "w\n"),
                ("z.txt", ""),
            ],
        )
        .await;
    let mount = fx.checkout("up", v1, "up").await;
    let dir = fx.checkout_dir("up");
    write(&dir, "x.txt", "1\n2\n3\n4\nfive\n");
    write(&dir, "y.txt", "local\n");

    let report = fx
        .rpc
        .update_mount(pb::UpdateMountRequest {
            name: mount.clone(),
            revision: v2.to_hex(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(report.merged, ["x.txt"]);
    assert_eq!(report.conflicts, ["y.txt"]);
    assert_eq!(report.updated, ["w.txt"]);
    assert_eq!(report.removed, ["z.txt"]);
    assert_eq!(read(&dir, "x.txt"), "one\n2\n3\n4\nfive\n");
    assert_eq!(read(&dir, "w.txt"), "w\n");
    assert!(!dir.join("z.txt").exists());
    assert!(read(&dir, "y.txt").contains("<<<<<<<"));
    assert_eq!(
        fx.status(&mount).await,
        vec![("x.txt".into(), MODIFIED), ("y.txt".into(), CONFLICT)]
    );

    let refused = fx
        .rpc
        .commit_mount(pb::CommitMountRequest {
            name: mount.clone(),
            message: "m".into(),
            new_change: true,
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert!(
        refused.to_string().contains("unresolved conflicts"),
        "{refused}"
    );
    let still_marked = fx
        .rpc
        .resolve_paths(pb::ResolvePathsRequest {
            name: mount.clone(),
            paths: vec!["y.txt".into()],
            force: false,
        })
        .await
        .unwrap_err();
    assert!(
        still_marked.to_string().contains("conflict markers"),
        "{still_marked}"
    );

    write(&dir, "y.txt", "both\n");
    fx.rpc
        .resolve_paths(pb::ResolvePathsRequest {
            name: mount.clone(),
            paths: vec!["y.txt".into()],
            force: false,
        })
        .await
        .unwrap();
    assert_eq!(
        fx.status(&mount).await,
        vec![("x.txt".into(), MODIFIED), ("y.txt".into(), MODIFIED)]
    );
    fx.rpc
        .commit_mount(pb::CommitMountRequest {
            name: mount.clone(),
            message: "m".into(),
            new_change: true,
            ..Default::default()
        })
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn edits_survive_a_restart_including_those_made_while_stopped() {
    let mut fx = Fixture::start().await;
    let v1 = fx
        .repo_with("rs", &[("a.txt", "a\n"), ("b.txt", "b\n")])
        .await;
    let mount = fx.checkout("rs", v1, "rs").await;
    let dir = fx.checkout_dir("rs");
    write(&dir, "a.txt", "edited\n");
    assert_eq!(fx.status(&mount).await, vec![("a.txt".into(), MODIFIED)]);
    fx.restart().await;
    write(&dir, "b.txt", "while stopped\n");
    // Give the restarted daemon's startup scan a moment, then status
    // settles whatever is left.
    assert_eq!(
        fx.status(&mount).await,
        vec![("a.txt".into(), MODIFIED), ("b.txt".into(), MODIFIED)]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn moving_the_base_keeping_files_turns_the_difference_into_edits() {
    let fx = Fixture::start().await;
    let v1 = fx.repo_with("keep", &[("a.txt", "1\n")]).await;
    let v2 = fx
        .version(
            "keep",
            Root::Version(v1),
            &[("a.txt", "2\n"), ("b.txt", "b\n")],
        )
        .await;
    let mount = fx.checkout("keep", v1, "keep").await;
    let dir = fx.checkout_dir("keep");
    write(&dir, "c.txt", "c\n");
    fx.rpc
        .update_mount(pb::UpdateMountRequest {
            name: mount.clone(),
            revision: v2.to_hex(),
            keep_files: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(read(&dir, "a.txt"), "1\n", "files stay as they were");
    assert!(!dir.join("b.txt").exists());
    assert_eq!(
        fx.status(&mount).await,
        vec![
            ("a.txt".into(), MODIFIED),
            ("b.txt".into(), DELETED),
            ("c.txt".into(), ADDED),
        ]
    );
    assert_eq!(fx.mount(&mount).await.base_version, v2.to_hex());
}
