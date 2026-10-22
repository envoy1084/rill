//! Import from a repository created with the `git` binary and read with
//! `gix`.

use std::path::Path;
use std::process::Command;

use bytes::Bytes;
use rill_core::{read_content, DeriveManager, RootTree, TreeOps};
use rill_git_import::{GixSource, ImportMap, ImportOptions, Importer};
use rill_store::{RepoStore, Stores};
use rill_types::{EntryKind, FileKind, RepoPathBuf, TreeHash, TrunkSeq};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Ada Lovelace")
        .env("GIT_AUTHOR_EMAIL", "ada@example.com")
        .env("GIT_COMMITTER_NAME", "Ada Lovelace")
        .env("GIT_COMMITTER_EMAIL", "ada@example.com")
        .env("GIT_COMMITTER_DATE", "2024-01-02T03:04:05Z")
        .env("GIT_AUTHOR_DATE", "2024-01-02T03:04:05Z")
        .output()
        .expect("git binary available");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn build_repo(dir: &Path) -> (String, String) {
    git(dir, &["init", "-q", "-b", "main"]);
    std::fs::write(dir.join("README.md"), "# hello\n").unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), "pub fn one() {}\n").unwrap();
    std::fs::write(dir.join("gone.txt"), "to be deleted\n").unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "first commit"]);
    let first = git(dir, &["rev-parse", "HEAD"]);

    std::fs::write(dir.join("src/lib.rs"), "pub fn one() {}\npub fn two() {}\n").unwrap();
    std::fs::remove_file(dir.join("gone.txt")).unwrap();
    std::fs::write(dir.join("run.sh"), "#!/bin/sh\necho hi\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.join("run.sh"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        std::os::unix::fs::symlink("README.md", dir.join("link")).unwrap();
    }
    git(dir, &["add", "."]);
    git(dir, &["update-index", "--chmod=+x", "run.sh"]);
    git(dir, &["commit", "-q", "-m", "second commit\n\nwith body"]);
    let second = git(dir, &["rev-parse", "HEAD"]);
    (first, second)
}

async fn repo() -> RepoStore {
    let stores = Stores::memory();
    let config = stores.create_repo("r", "").await.unwrap();
    stores.repo(config.id)
}

async fn file(
    repo: &RepoStore,
    trees: &TreeOps,
    root: TreeHash,
    path: &str,
) -> Option<(Bytes, FileKind)> {
    match trees
        .lookup(root, &RepoPathBuf::new(path).unwrap())
        .await
        .unwrap()?
    {
        EntryKind::File { content, kind } => Some((
            read_content(repo.objects().as_ref(), &content)
                .await
                .unwrap(),
            kind,
        )),
        EntryKind::Dir { .. } => None,
    }
}

#[tokio::test]
async fn imports_real_history() {
    let dir = tempfile::tempdir().unwrap();
    let (first, second) = build_repo(dir.path());

    let repo = repo().await;
    let mut importer = Importer::open(repo.clone()).await.unwrap();
    let source = GixSource::open(dir.path()).unwrap();
    let report = importer
        .import(source, "HEAD", ImportOptions::default())
        .await
        .unwrap();
    assert_eq!(report.imported, 2);
    assert_eq!(report.skipped, 0);
    assert_eq!(report.landed, 2);
    assert_eq!(report.seq, Some(TrunkSeq(2)));
    assert!(report.warnings.is_empty());

    let head = report.head.unwrap();
    assert_eq!(repo.trunk_head().await.unwrap(), Some((TrunkSeq(2), head)));
    let cv = repo.get_change_version(&head).await.unwrap().unwrap();
    assert_eq!(cv.message, "second commit\n\nwith body\n");
    assert_eq!(cv.author.name, "Ada Lovelace");
    assert_eq!(cv.author.identity.as_deref(), Some("ada@example.com"));
    assert_eq!(cv.created_at.to_rfc3339(), "2024-01-02T03:04:05Z");
    assert_eq!(hex::encode(&cv.extra["git_sha"]), second);
    assert_eq!(cv.parents.len(), 1);
    let parent = repo
        .get_change_version(&cv.parents[0])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(hex::encode(&parent.extra["git_sha"]), first);
    assert!(parent.parents.is_empty());
    assert_eq!(
        repo.trunk_seq_of(&cv.parents[0]).await.unwrap(),
        Some(TrunkSeq(1))
    );

    let trees = TreeOps::new(repo.objects().clone(), 16);
    let derive = DeriveManager::new(repo.clone(), trees.clone(), 16);
    let root = derive.get::<RootTree>(&head).await.unwrap();
    assert_eq!(
        file(&repo, &trees, root, "src/lib.rs").await.unwrap(),
        (
            Bytes::from_static(b"pub fn one() {}\npub fn two() {}\n"),
            FileKind::Regular
        )
    );
    assert_eq!(
        file(&repo, &trees, root, "run.sh").await.unwrap(),
        (
            Bytes::from_static(b"#!/bin/sh\necho hi\n"),
            FileKind::Executable
        )
    );
    assert!(file(&repo, &trees, root, "gone.txt").await.is_none());
    #[cfg(unix)]
    assert_eq!(
        file(&repo, &trees, root, "link").await.unwrap(),
        (Bytes::from_static(b"README.md"), FileKind::Symlink)
    );
    let first_root = derive.get::<RootTree>(&cv.parents[0]).await.unwrap();
    assert!(file(&repo, &trees, first_root, "gone.txt").await.is_some());
    assert!(file(&repo, &trees, first_root, "run.sh").await.is_none());

    // Re-import is a no-op, including through a fresh importer.
    let mut again = Importer::open(repo.clone()).await.unwrap();
    let report = again
        .import(
            GixSource::open(dir.path()).unwrap(),
            "main",
            ImportOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(report.imported, 0);
    assert_eq!(report.skipped, 2);
    assert_eq!(report.landed, 0);
    assert_eq!(report.seq, Some(TrunkSeq(2)));
    assert_eq!(ImportMap::load(&repo).await.unwrap().len(), 2);

    // A raw sha works as the head ref too.
    let report = again
        .import(
            GixSource::open(dir.path()).unwrap(),
            &first,
            ImportOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(report.skipped, 1);
    assert_eq!(report.head, Some(cv.parents[0]));
}
