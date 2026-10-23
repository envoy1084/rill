//! Lazy and snapshot mounts through FUSE (Linux): listings without
//! fetching, reads through the cache, writes into the overlay.

#![cfg(target_os = "linux")]

mod common;

use std::os::unix::fs::PermissionsExt;

use common::{read, write, Fixture};
use rill_daemon::proto as pb;

const ADDED: pb::ChangeKind = pb::ChangeKind::Added;
const MODIFIED: pb::ChangeKind = pb::ChangeKind::Modified;
const DELETED: pb::ChangeKind = pb::ChangeKind::Deleted;

/// FUSE needs `/dev/fuse` and `fusermount3`; skip where they are missing.
fn fuse_available() -> bool {
    std::path::Path::new("/dev/fuse").exists()
        && std::process::Command::new("fusermount3")
            .arg("--version")
            .output()
            .is_ok()
}

#[tokio::test(flavor = "multi_thread")]
async fn listing_does_not_fetch_and_edits_land_in_the_overlay() {
    if !fuse_available() {
        eprintln!("no FUSE here; skipped");
        return;
    }
    let fx = Fixture::start().await;
    let big = "x".repeat(4096);
    let v1 = fx
        .repo_with(
            "lz",
            &[
                ("a.txt", &big),
                ("src/lib.rs", "fn a() {}\n"),
                ("src/old.rs", "old\n"),
            ],
        )
        .await;
    let mount = fx.checkout_as("lz", v1, "lz", pb::MountMode::Lazy).await;
    let dir = fx.checkout_dir("lz");

    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, [".rill", "a.txt", "src"]);
    assert_eq!(std::fs::metadata(dir.join("a.txt")).unwrap().len(), 4096);
    let misses = fx.cache().await.misses;
    assert_eq!(read(&dir, "a.txt"), big);
    assert!(fx.cache().await.misses > misses, "the first read fetched");
    assert!(read(&dir, ".rill/mount.json").contains(&mount));

    write(&dir, "src/lib.rs", "fn b() {}\n");
    write(&dir, "new/deep.txt", "n\n");
    std::fs::remove_file(dir.join("src/old.rs")).unwrap();
    std::fs::rename(dir.join("a.txt"), dir.join("moved.txt")).unwrap();
    let mut perms = std::fs::metadata(dir.join("new/deep.txt"))
        .unwrap()
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(dir.join("new/deep.txt"), perms).unwrap();

    assert_eq!(read(&dir, "src/lib.rs"), "fn b() {}\n");
    assert_eq!(read(&dir, "moved.txt"), big);
    assert!(!dir.join("src/old.rs").exists());
    assert_eq!(
        fx.status(&mount).await,
        vec![
            ("a.txt".into(), DELETED),
            ("moved.txt".into(), ADDED),
            ("new/deep.txt".into(), ADDED),
            ("src/lib.rs".into(), MODIFIED),
            ("src/old.rs".into(), DELETED),
        ]
    );
    let committed = fx
        .rpc
        .commit_mount(pb::CommitMountRequest {
            name: mount.clone(),
            message: "lazy".into(),
            new_change: true,
            ..Default::default()
        })
        .await
        .unwrap();
    let hash = committed.version.parse().unwrap();
    let kind = fx
        .repo("lz")
        .resolve_path(
            &rill_sdk::Root::Version(hash),
            &common::path("new/deep.txt"),
        )
        .await
        .unwrap();
    assert!(
        matches!(
            kind,
            Some(rill_types::EntryKind::File {
                kind: rill_types::FileKind::Executable,
                ..
            })
        ),
        "{kind:?}"
    );
    fx.rpc
        .delete_mount(pb::DeleteMountRequest { name: mount })
        .await
        .unwrap();
    assert!(!dir.join("src").exists(), "deleting the mount unmounts it");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_snapshot_mount_reads_a_version_and_keeps_writes_to_itself() {
    if !fuse_available() {
        eprintln!("no FUSE here; skipped");
        return;
    }
    let fx = Fixture::start().await;
    let v1 = fx.repo_with("ci", &[("build.sh", "make\n")]).await;
    let mount = fx
        .checkout_as("ci", v1, "ci", pb::MountMode::Snapshot)
        .await;
    let dir = fx.checkout_dir("ci");
    assert_eq!(read(&dir, "build.sh"), "make\n");
    write(&dir, "out/artifact", "built\n");
    assert_eq!(read(&dir, "out/artifact"), "built\n");
    let m = fx.mount(&mount).await;
    assert!(m.workspace.is_empty(), "a snapshot mount has no workspace");
    fx.rpc
        .delete_mount(pb::DeleteMountRequest { name: mount })
        .await
        .unwrap();
    assert!(!dir.join("build.sh").exists());
}
