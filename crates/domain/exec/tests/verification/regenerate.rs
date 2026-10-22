//! Merge drivers' regenerate commands: run on the merged tree, their
//! outputs read back, failures explained.

use std::time::Duration;

use rill_exec::verify::Regeneration;
use rill_types::RepoPathBuf;

use crate::fixture::Fixture;

fn job(command: &str) -> Regeneration {
    Regeneration {
        name: "deps".into(),
        command: vec!["sh".into(), "-c".into(), command.into()],
        inputs: vec!["deps/**".into(), "deps.lock".into()],
        outputs: vec![RepoPathBuf::new("deps.lock").unwrap()],
        env: Default::default(),
        platform: Default::default(),
        timeout: Duration::from_secs(30),
    }
}

#[tokio::test]
async fn a_regeneration_returns_the_files_it_rebuilt() {
    let fx = Fixture::new().await;
    let root = fx
        .tree(
            None,
            &[
                ("deps/a", "1"),
                ("deps/b", "2"),
                ("deps.lock", "stale\n"),
                ("src/x", ""),
            ],
        )
        .await;
    let files = fx
        .runner
        .regenerate(root, &job("ls deps > deps.lock; test ! -e src"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        files
            .get(&RepoPathBuf::new("deps.lock").unwrap())
            .map(|b| b.as_ref()),
        Some(&b"a\nb\n"[..]),
        "it sees only its inputs"
    );
}

#[tokio::test]
async fn a_failed_or_silent_regeneration_says_why() {
    let fx = Fixture::new().await;
    let root = fx.tree(None, &[("deps/a", "1")]).await;
    let failed = fx
        .runner
        .regenerate(root, &job("echo no network >&2; exit 2"))
        .await
        .unwrap()
        .unwrap_err();
    assert!(
        failed.starts_with("failed with exit code 2") && failed.contains("no network"),
        "{failed}"
    );
    let silent = fx
        .runner
        .regenerate(root, &job("true"))
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(silent, "it did not write deps.lock");
}
