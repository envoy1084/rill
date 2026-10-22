//! Local checkout: create, status, ignore rules, snapshot and update.

mod common;

use common::{fake_hash, Harness, MockFile, MockRepo};
use rill_sdk::{ApplyOptions, Checkout, Edit, LocalChangeKind, Root};
use rill_types::{FileKind, RepoPathBuf, SparseProfile};

fn p(s: &str) -> RepoPathBuf {
    RepoPathBuf::new(s).unwrap()
}

fn base_files() -> Vec<(&'static str, MockFile)> {
    vec![
        ("README.md", MockFile::regular("# hi\n")),
        ("src/lib.rs", MockFile::regular("fn a() {}\n")),
        ("src/util.rs", MockFile::regular("fn u() {}\n")),
        ("docs/guide.md", MockFile::regular("guide\n")),
        (
            "bin/run",
            MockFile {
                bytes: "#!/bin/sh\n".into(),
                kind: FileKind::Executable,
            },
        ),
    ]
}

/// A mock whose trunk points at version `base` and serves the same tree
/// under both labels, so pinned base reads work.
fn mock_with_base() -> MockRepo {
    let mock = MockRepo::new();
    let base = fake_hash("base");
    mock.set_trunk(1, base);
    mock.set_tree("trunk", base_files());
    mock.set_tree(&base.to_hex(), base_files());
    mock
}

#[tokio::test]
async fn create_materializes_and_status_tracks_changes() {
    let h = Harness::with_repo(mock_with_base()).await;
    let dir = tempfile::tempdir().unwrap();
    let repo = h.client.repo("main");
    let mut co = Checkout::create(dir.path(), repo, Root::trunk(), SparseProfile::full())
        .await
        .unwrap();

    assert_eq!(co.manifest().len(), 5);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("src/lib.rs")).unwrap(),
        "fn a() {}\n"
    );
    assert_eq!(co.meta().base_version, Some(fake_hash("base")));
    assert!(dir.path().join(".rill/checkout.toml").exists());
    assert!(co.status().await.unwrap().is_empty());

    std::fs::write(dir.path().join("src/lib.rs"), "fn a() { changed }\n").unwrap();
    std::fs::remove_file(dir.path().join("docs/guide.md")).unwrap();
    std::fs::write(dir.path().join("src/new.rs"), "new\n").unwrap();

    let status = co.status().await.unwrap();
    let got: Vec<_> = status.iter().map(|c| (c.path.as_str(), c.kind)).collect();
    assert_eq!(
        got,
        vec![
            ("docs/guide.md", LocalChangeKind::Deleted),
            ("src/lib.rs", LocalChangeKind::Modified),
            ("src/new.rs", LocalChangeKind::Added),
        ]
    );

    // Edits mirror status; snapshot sends them and remembers the change.
    let edits = co.edits().await.unwrap();
    assert!(matches!(&edits[0], Edit::Delete { path } if path == &p("docs/guide.md")));
    assert!(matches!(&edits[1], Edit::Set { path, bytes, .. }
        if path == &p("src/lib.rs") && &bytes[..] == b"fn a() { changed }\n"));
    let v1 = co.snapshot(ApplyOptions::message("wip")).await.unwrap();
    assert_eq!(co.meta().working_change, Some(v1.version.change_id));
    let v2 = co.snapshot(ApplyOptions::message("wip 2")).await.unwrap();
    assert_eq!(v2.version.change_id, v1.version.change_id);

    // Reopen from disk and see the same state.
    let reopened = Checkout::open(dir.path(), h.client.clone()).await.unwrap();
    assert_eq!(reopened.meta(), co.meta());
    assert_eq!(reopened.status().await.unwrap().len(), 3);
    assert_eq!(
        Checkout::find(dir.path().join("src")),
        Some(dir.path().to_path_buf())
    );
}

#[tokio::test]
async fn sparse_profile_limits_materialization() {
    let h = Harness::with_repo(mock_with_base()).await;
    let dir = tempfile::tempdir().unwrap();
    let sparse = SparseProfile {
        include: vec![p("src")],
        exclude: vec![p("src/util.rs")],
    };
    let co = Checkout::create(dir.path(), h.client.repo("main"), Root::trunk(), sparse)
        .await
        .unwrap();
    let paths: Vec<_> = co.manifest().keys().map(|k| k.as_str()).collect();
    assert_eq!(paths, vec!["src/lib.rs"]);
    assert!(!dir.path().join("README.md").exists());
}

#[tokio::test]
async fn rillignore_hides_untracked_but_not_tracked_files() {
    let h = Harness::with_repo(mock_with_base()).await;
    let dir = tempfile::tempdir().unwrap();
    let co = Checkout::create(
        dir.path(),
        h.client.repo("main"),
        Root::trunk(),
        SparseProfile::full(),
    )
    .await
    .unwrap();

    std::fs::write(dir.path().join(".rillignore"), "target/\n*.log\n").unwrap();
    std::fs::create_dir_all(dir.path().join("target/debug")).unwrap();
    std::fs::write(dir.path().join("target/debug/out"), "x").unwrap();
    std::fs::write(dir.path().join("build.log"), "x").unwrap();
    std::fs::write(dir.path().join("keep.txt"), "x").unwrap();
    // Nested ignore files apply to their own directory.
    std::fs::write(dir.path().join("src/.rillignore"), "*.tmp\n").unwrap();
    std::fs::write(dir.path().join("src/scratch.tmp"), "x").unwrap();
    // A tracked file matching an ignore rule is still reported.
    std::fs::write(dir.path().join("docs/.rillignore"), "guide.md\n").unwrap();
    std::fs::write(dir.path().join("docs/guide.md"), "edited\n").unwrap();

    let status = co.status().await.unwrap();
    let got: Vec<_> = status.iter().map(|c| (c.path.as_str(), c.kind)).collect();
    assert_eq!(
        got,
        vec![
            ("docs/guide.md", LocalChangeKind::Modified),
            ("keep.txt", LocalChangeKind::Added),
        ]
    );
}

#[tokio::test]
async fn update_preserves_local_work_and_merges() {
    let mock = mock_with_base();
    let new = fake_hash("new");
    mock.set_tree(
        &new.to_hex(),
        vec![
            // Unchanged upstream; locally modified below → kept.
            ("README.md", MockFile::regular("# hi\n")),
            // Changed upstream on a different line than the local edit → clean merge.
            ("src/lib.rs", MockFile::regular("fn a() {}\nfn b() {}\n")),
            // Changed upstream on the same line as the local edit → conflict.
            ("src/util.rs", MockFile::regular("fn u() { upstream }\n")),
            // docs/guide.md removed upstream, untouched locally → removed.
            // bin/run unchanged.
            (
                "bin/run",
                MockFile {
                    bytes: "#!/bin/sh\n".into(),
                    kind: FileKind::Executable,
                },
            ),
            // Added upstream.
            ("src/added.rs", MockFile::regular("added\n")),
        ],
    );
    let h = Harness::with_repo(mock).await;
    let dir = tempfile::tempdir().unwrap();
    let mut co = Checkout::create(
        dir.path(),
        h.client.repo("main"),
        Root::trunk(),
        SparseProfile::full(),
    )
    .await
    .unwrap();

    std::fs::write(dir.path().join("README.md"), "# hi\nlocal\n").unwrap();
    std::fs::write(dir.path().join("src/lib.rs"), "// local\nfn a() {}\n").unwrap();
    std::fs::write(dir.path().join("src/util.rs"), "fn u() { local }\n").unwrap();

    let report = co.update(Root::Version(new)).await.unwrap();
    assert_eq!(report.updated, vec![p("src/added.rs")]);
    assert_eq!(report.removed, vec![p("docs/guide.md")]);
    assert_eq!(report.merged, vec![p("src/lib.rs")]);
    assert_eq!(report.conflicts, vec![p("src/util.rs")]);

    assert_eq!(
        std::fs::read_to_string(dir.path().join("README.md")).unwrap(),
        "# hi\nlocal\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("src/lib.rs")).unwrap(),
        "// local\nfn a() {}\nfn b() {}\n"
    );
    let conflicted = std::fs::read_to_string(dir.path().join("src/util.rs")).unwrap();
    assert!(conflicted.contains("<<<<<<<"), "{conflicted}");
    assert!(conflicted.contains("local") && conflicted.contains("upstream"));
    assert!(!dir.path().join("docs").exists());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("src/added.rs")).unwrap(),
        "added\n"
    );

    assert_eq!(co.base(), &Root::Version(new));
    assert_eq!(co.meta().base_version, Some(new));
    let status = co.status().await.unwrap();
    let got: Vec<_> = status.iter().map(|c| (c.path.as_str(), c.kind)).collect();
    assert_eq!(
        got,
        vec![
            ("README.md", LocalChangeKind::Modified),
            ("src/lib.rs", LocalChangeKind::Modified),
            ("src/util.rs", LocalChangeKind::Conflict),
        ]
    );
    assert!(co.edits().await.is_err());

    let err = co
        .mark_resolved(&[p("src/util.rs")], false)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("still contains conflict markers"),
        "{err}"
    );
    let truncated = conflicted.lines().take(2).collect::<Vec<_>>().join("\n");
    std::fs::write(dir.path().join("src/util.rs"), truncated).unwrap();
    let err = co
        .mark_resolved(&[p("src/util.rs")], false)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("malformed"), "{err}");
    assert_eq!(
        co.status().await.unwrap()[2].kind,
        LocalChangeKind::Conflict
    );

    std::fs::write(dir.path().join("src/util.rs"), "fn u() { both }\n").unwrap();
    co.mark_resolved(&[p("src/util.rs")], false).await.unwrap();
    let status = co.status().await.unwrap();
    assert_eq!(status[2].kind, LocalChangeKind::Modified);
    assert_eq!(co.edits().await.unwrap().len(), 3);
}

#[tokio::test]
async fn update_without_local_changes_is_a_plain_switch() {
    let mock = mock_with_base();
    let new = fake_hash("new2");
    mock.set_tree(
        &new.to_hex(),
        vec![
            ("README.md", MockFile::regular("# v2\n")),
            ("only.txt", MockFile::regular("x")),
        ],
    );
    let h = Harness::with_repo(mock).await;
    let dir = tempfile::tempdir().unwrap();
    let mut co = Checkout::create(
        dir.path(),
        h.client.repo("main"),
        Root::trunk(),
        SparseProfile::full(),
    )
    .await
    .unwrap();
    let report = co.update(Root::Version(new)).await.unwrap();
    assert_eq!(report.updated.len(), 2);
    assert_eq!(report.removed.len(), 4);
    assert!(report.conflicts.is_empty());
    assert!(co.status().await.unwrap().is_empty());
    assert_eq!(co.manifest().len(), 2);
    assert!(!dir.path().join("src").exists());
}

#[tokio::test]
async fn update_merges_adjacent_additions_in_source_files() {
    let mock = mock_with_base();
    let new = fake_hash("new");
    mock.set_tree(
        &new.to_hex(),
        vec![
            ("README.md", MockFile::regular("# hi\n")),
            (
                "src/lib.rs",
                MockFile::regular("fn a() {}\n\nfn upstream() {}\n"),
            ),
            ("src/util.rs", MockFile::regular("fn u() {}\n")),
            ("docs/guide.md", MockFile::regular("guide\n")),
            (
                "bin/run",
                MockFile {
                    bytes: "#!/bin/sh\n".into(),
                    kind: FileKind::Executable,
                },
            ),
        ],
    );
    let h = Harness::with_repo(mock).await;
    let dir = tempfile::tempdir().unwrap();
    let mut co = Checkout::create(
        dir.path(),
        h.client.repo("main"),
        Root::trunk(),
        SparseProfile::full(),
    )
    .await
    .unwrap();
    std::fs::write(
        dir.path().join("src/lib.rs"),
        "fn a() {}\n\nfn local() {}\n",
    )
    .unwrap();

    let report = co.update(Root::Version(new)).await.unwrap();
    assert_eq!(report.merged, vec![p("src/lib.rs")], "{report:?}");
    assert!(report.conflicts.is_empty(), "{report:?}");
    let text = std::fs::read_to_string(dir.path().join("src/lib.rs")).unwrap();
    assert!(
        text.contains("fn local()") && text.contains("fn upstream()"),
        "{text}"
    );
}
