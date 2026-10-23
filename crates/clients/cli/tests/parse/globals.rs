//! Global flags, completions, import, watch and exit codes.

use clap::Parser;
use rill_cli::cli::{Cli, Command};
use rill_cli::commands::watch;

use super::parse;

#[test]
fn global_flags_anywhere() {
    let cli = parse(&[
        "--json",
        "--endpoint",
        "http://h:1",
        "repo",
        "list",
        "--repo",
        "r",
        "-C",
        "/tmp",
    ]);
    assert!(cli.global.json);
    assert_eq!(cli.global.endpoint.as_deref(), Some("http://h:1"));
    assert_eq!(cli.global.repo.as_deref(), Some("r"));
    assert_eq!(
        cli.global.cwd.as_deref(),
        Some(std::path::Path::new("/tmp"))
    );
    let cli = parse(&["-vv", "status"]);
    assert_eq!(cli.global.verbose, 2);
    assert!(matches!(cli.command, Command::Status(_)));
}

#[test]
fn import_watch_completions() {
    match parse(&[
        "import",
        "git",
        "/src/repo",
        "--repo",
        "main",
        "--ref",
        "main",
    ])
    .command
    {
        Command::Import(rill_cli::commands::import::ImportCmd::Git(a)) => {
            assert_eq!(a.repo, "main");
            assert_eq!(a.r#ref, "main");
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        parse(&["watch", "trunk", "--after", "3"]).command,
        Command::Watch(watch::WatchCmd::Trunk(_))
    ));
    assert!(matches!(
        parse(&["watch", "refs", "release/"]).command,
        Command::Watch(watch::WatchCmd::Refs(_))
    ));
    assert!(matches!(
        parse(&["watch", "log"]).command,
        Command::Watch(watch::WatchCmd::Log(_))
    ));
    assert!(matches!(
        parse(&["completions", "bash"]).command,
        Command::Completions(_)
    ));
    assert!(Cli::try_parse_from(["rill", "completions", "nope"]).is_err());
}

#[tokio::test]
async fn help_and_usage_exit_codes() {
    let mut out = Vec::new();
    assert_eq!(
        rill_cli::run_cli_with_args(["rill", "--help"], &mut out).await,
        0
    );
    assert!(String::from_utf8(out).unwrap().contains("Usage"));
    let mut out = Vec::new();
    assert_eq!(
        rill_cli::run_cli_with_args(["rill", "nonsense"], &mut out).await,
        2
    );
    let mut out = Vec::new();
    assert_eq!(
        rill_cli::run_cli_with_args(["rill", "completions", "zsh"], &mut out).await,
        0
    );
    assert!(String::from_utf8(out).unwrap().contains("rill"));
    // A checkout command outside a checkout is a usage error.
    let dir = tempfile::tempdir().unwrap();
    let mut out = Vec::new();
    let code = rill_cli::run_cli_with_args(
        [
            "rill",
            "-C",
            dir.path().to_str().unwrap(),
            "--endpoint",
            "http://127.0.0.1:1",
            "status",
        ],
        &mut out,
    )
    .await;
    assert_eq!(code, 2);
}
