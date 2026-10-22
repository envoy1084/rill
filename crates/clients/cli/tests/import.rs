//! `rill import git` into a running server, through its API.

mod common;

use std::path::Path;
use std::process::Command;

use common::Server;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Ada Lovelace")
        .env("GIT_AUTHOR_EMAIL", "ada@example.com")
        .env("GIT_COMMITTER_NAME", "Ada Lovelace")
        .env("GIT_COMMITTER_EMAIL", "ada@example.com")
        .output()
        .expect("git binary available");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn git_history_round_trips_and_a_following_mirror_keeps_up() {
    let server = Server::start().await;
    let work = tempfile::tempdir().unwrap();
    let src = work.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    git(&src, &["init", "-q", "-b", "main"]);
    std::fs::write(src.join("README.md"), "# hello\n").unwrap();
    git(&src, &["add", "."]);
    git(&src, &["commit", "-q", "-m", "first"]);
    std::fs::write(src.join("lib.rs"), "pub fn one() {}\n").unwrap();
    git(&src, &["add", "."]);
    git(&src, &["commit", "-q", "-m", "second"]);

    let path = src.display().to_string();
    let mirror = work.path().join("mirror");
    let mirror_path = mirror.display().to_string();
    let export = ["--repo", "imported", "export", "git", mirror_path.as_str()];
    let first = server
        .json(work.path(), &["import", "git", &path, "--repo", "imported"])
        .await;
    assert_eq!(first["imported"], 2, "{first}");
    assert_eq!(first["landed"], 2, "{first}");
    assert_eq!(first["seq"], 2, "{first}");
    let exported = server.json(work.path(), &export).await;
    assert_eq!(exported["exported"], 2, "{exported}");
    assert_eq!(
        git(&mirror, &["rev-parse", "trunk^{tree}"]),
        git(&src, &["rev-parse", "HEAD^{tree}"]),
        "the mirror holds exactly the git tree"
    );

    std::fs::write(src.join("lib.rs"), "pub fn two() {}\n").unwrap();
    git(&src, &["commit", "-q", "-am", "third"]);
    let again = server
        .json(work.path(), &["import", "git", &path, "--repo", "imported"])
        .await;
    assert_eq!(again["imported"], 1, "only the new commit: {again}");
    assert_eq!(again["skipped"], 2, "{again}");
    assert_eq!(again["seq"], 3, "{again}");

    let file = server
        .ok(work.path(), &["--repo", "imported", "cat", "lib.rs"])
        .await;
    assert_eq!(file, "pub fn two() {}\n");

    let exported = server.json(work.path(), &export).await;
    assert_eq!(exported["exported"], 1, "only the new position: {exported}");
    assert_eq!(git(&mirror, &["rev-list", "--count", "trunk"]), "3");
    assert_eq!(
        git(&mirror, &["rev-parse", "trunk^{tree}"]),
        git(&src, &["rev-parse", "HEAD^{tree}"])
    );
    assert!(git(&mirror, &["log", "-1", "--format=%an <%ae>", "trunk"]).contains("Ada Lovelace"));

    let mut follow = export.to_vec();
    follow.push("--follow");
    let following = server.rill(work.path(), &follow);
    let landing = async {
        std::fs::write(src.join("lib.rs"), "pub fn three() {}\n").unwrap();
        git(&src, &["commit", "-q", "-am", "fourth"]);
        server
            .json(work.path(), &["import", "git", &path, "--repo", "imported"])
            .await;
        for _ in 0..200 {
            if git(&mirror, &["rev-list", "--count", "trunk"]) == "4" {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        panic!("the following mirror never exported trunk@4");
    };
    tokio::select! {
        (code, out) = following => panic!("--follow exited ({code}): {out}"),
        () = landing => {}
    }
    assert_eq!(
        git(&mirror, &["rev-parse", "trunk^{tree}"]),
        git(&src, &["rev-parse", "HEAD^{tree}"]),
        "a following mirror keeps the git tree"
    );
}
