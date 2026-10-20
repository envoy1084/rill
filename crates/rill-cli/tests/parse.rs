//! Command-line parsing for every command group.

use clap::Parser;
use rill_cli::cli::{Cli, Command};
use rill_cli::commands::{admin, intent, land, refs, repo, review, stack, watch, ws};

fn parse(args: &[&str]) -> Cli {
    let mut full = vec!["rill"];
    full.extend_from_slice(args);
    Cli::try_parse_from(full).unwrap_or_else(|e| panic!("{args:?}: {e}"))
}

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
fn repo_group() {
    assert!(
        matches!(parse(&["repo", "list", "--limit", "3"]).command, Command::Repo(repo::RepoCmd::List(a)) if a.limit == 3)
    );
    match parse(&["repo", "create", "main", "--description", "d"]).command {
        Command::Repo(repo::RepoCmd::Create(a)) => {
            assert_eq!(a.name, "main");
            assert_eq!(a.description.as_deref(), Some("d"));
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        parse(&["repo", "info"]).command,
        Command::Repo(repo::RepoCmd::Info)
    ));
}

#[test]
fn checkout_and_local_commands() {
    match parse(&[
        "checkout", "main", "dir", "--at", "trunk@3", "--sparse", "src", "--sparse", "docs",
    ])
    .command
    {
        Command::Checkout(a) => {
            assert_eq!(a.repo, "main");
            assert_eq!(a.at.as_deref(), Some("trunk@3"));
            assert_eq!(a.sparse, vec!["src", "docs"]);
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(parse(&["status", "src"]).command, Command::Status(a) if a.paths == ["src"]));
    assert!(
        matches!(parse(&["diff", "--base", "trunk@1", "--target", "trunk", "a", "b"]).command, Command::Diff(a) if a.paths.len() == 2 && a.target.is_some())
    );
    assert!(
        matches!(parse(&["log", "-n", "5", "--path", "x", "--first-parent", "trunk"]).command, Command::Log(a) if a.limit == 5 && a.first_parent)
    );
    assert!(matches!(
        parse(&["blame", "src/a.rs", "--at", "trunk@2"]).command,
        Command::Blame(_)
    ));
    assert!(matches!(
        parse(&["show", "ch_00"]).command,
        Command::Show(_)
    ));
    assert!(matches!(parse(&["ls"]).command, Command::Ls(a) if a.path.is_none()));
    assert!(matches!(
        parse(&["cat", "README.md"]).command,
        Command::Cat(_)
    ));
    assert!(
        matches!(parse(&["grep", "-i", "fn", "--glob", "*.rs"]).command, Command::Grep(a) if a.ignore_case)
    );
    assert!(
        matches!(parse(&["new", "-m", "msg", "--intent", "in_00"]).command, Command::New(a) if a.message == "msg")
    );
    assert!(matches!(
        parse(&["describe", "-m", "m"]).command,
        Command::Describe(_)
    ));
    assert!(matches!(parse(&["amend"]).command, Command::Amend(a) if a.message.is_none()));
    assert!(matches!(
        parse(&["sync", "--onto", "trunk@4"]).command,
        Command::Sync(_)
    ));
    assert!(matches!(parse(&["resolve", "--list"]).command, Command::Resolve(a) if a.list));
    assert!(
        matches!(parse(&["resolve", "a", "b"]).command, Command::Resolve(a) if a.paths.len() == 2)
    );
    assert!(matches!(parse(&["undo"]).command, Command::Undo));
    assert!(
        Cli::try_parse_from(["rill", "new"]).is_err(),
        "new requires -m"
    );
}

#[test]
fn stack_submit_review() {
    assert!(matches!(
        parse(&["stack", "show"]).command,
        Command::Stack(stack::StackCmd::Show(_))
    ));
    assert!(matches!(
        parse(&["stack", "sync", "--onto", "trunk"]).command,
        Command::Stack(stack::StackCmd::Sync(_))
    ));
    assert!(matches!(
        parse(&["stack", "submit"]).command,
        Command::Stack(stack::StackCmd::Submit(_))
    ));
    assert!(
        matches!(parse(&["stack", "land", "--wait", "--timeout", "9"]).command, Command::Stack(stack::StackCmd::Land(a)) if a.timeout == 9)
    );
    assert!(matches!(parse(&["submit"]).command, Command::Submit(a) if a.change.is_none()));
    assert!(matches!(
        parse(&["review", "show", "ch_00"]).command,
        Command::Review(review::ReviewCmd::Show(_))
    ));
    assert!(matches!(
        parse(&["review", "list", "--status", "open", "--owner", "pr_00"]).command,
        Command::Review(review::ReviewCmd::List(_))
    ));
    match parse(&["review", "vote", "ch_00", "Review", "-2", "-m", "no"]).command {
        Command::Review(review::ReviewCmd::Vote(a)) => {
            assert_eq!(a.value, "-2");
            assert_eq!(a.label, "Review");
        }
        other => panic!("{other:?}"),
    }
    assert!(
        matches!(parse(&["review", "vote", "ch_00", "Review", "+2"]).command, Command::Review(review::ReviewCmd::Vote(a)) if a.value == "+2")
    );
    assert!(
        matches!(parse(&["review", "comment", "ch_00", "-m", "hi", "--path", "a", "--line", "3"]).command, Command::Review(review::ReviewCmd::Comment(a)) if a.line == Some(3))
    );
    assert!(matches!(
        parse(&["review", "threads"]).command,
        Command::Review(review::ReviewCmd::Threads(_))
    ));
    assert!(
        matches!(parse(&["review", "resolve-thread", "ch_00", "t1", "--reopen"]).command, Command::Review(review::ReviewCmd::ResolveThread(a)) if a.reopen)
    );
    assert!(matches!(
        parse(&["review", "evaluate", "ch_00"]).command,
        Command::Review(review::ReviewCmd::Evaluate(_))
    ));
    assert!(matches!(
        parse(&["review", "abandon", "ch_00", "-m", "why"]).command,
        Command::Review(review::ReviewCmd::Abandon(_))
    ));
}

#[test]
fn landing_group() {
    assert!(matches!(
        parse(&["land", "ch_00", "--wait", "--timeout", "5"]).command,
        Command::Land(land::LandArgs {
            wait: true,
            timeout: 5,
            ..
        })
    ));
    assert!(matches!(parse(&["trunk"]).command, Command::Trunk));
    assert!(
        matches!(parse(&["landings", "-n", "3", "--before", "trunk@9"]).command, Command::Landings(a) if a.limit == 3)
    );
    assert!(matches!(
        parse(&["verify", "7"]).command,
        Command::Verify(_)
    ));
    assert!(matches!(
        parse(&["revert", "7", "-m", "bad"]).command,
        Command::Revert(_)
    ));
    assert!(
        matches!(parse(&["bisect", "3", "9", "--target", "tests"]).command, Command::Bisect(a) if a.target == "tests")
    );
}

#[test]
fn intent_ws_ref_admin() {
    assert!(matches!(
        parse(&["intent", "create", "t", "--kind", "bug", "--body", "b", "--parent", "in_00"])
            .command,
        Command::Intent(intent::IntentCmd::Create(_))
    ));
    assert!(matches!(
        parse(&["intent", "list", "--status", "open"]).command,
        Command::Intent(intent::IntentCmd::List(_))
    ));
    assert!(matches!(
        parse(&["intent", "show", "in_00"]).command,
        Command::Intent(intent::IntentCmd::Show(_))
    ));
    assert!(
        matches!(parse(&["intent", "update", "in_00", "--status", "done", "--no-parent"]).command, Command::Intent(intent::IntentCmd::Update(a)) if a.no_parent)
    );
    assert!(Cli::try_parse_from([
        "rill",
        "intent",
        "update",
        "in_00",
        "--parent",
        "x",
        "--no-parent"
    ])
    .is_err());
    assert!(matches!(
        parse(&["intent", "link", "in_00", "ch_00"]).command,
        Command::Intent(intent::IntentCmd::Link(_))
    ));
    assert!(matches!(
        parse(&["intent", "similar", "text"]).command,
        Command::Intent(intent::IntentCmd::Similar(_))
    ));

    assert!(matches!(
        parse(&["ws", "create", "n", "--at", "trunk"]).command,
        Command::Ws(ws::WsCmd::Create(_))
    ));
    assert!(matches!(
        parse(&["ws", "list"]).command,
        Command::Ws(ws::WsCmd::List(_))
    ));
    assert!(matches!(
        parse(&["ws", "show"]).command,
        Command::Ws(ws::WsCmd::Show(_))
    ));
    assert!(matches!(
        parse(&["ws", "handoff", "ws_00", "agent:bot"]).command,
        Command::Ws(ws::WsCmd::Handoff(_))
    ));
    assert!(matches!(
        parse(&["ws", "ops", "ws_00", "-n", "2"]).command,
        Command::Ws(ws::WsCmd::Ops(_))
    ));
    assert!(matches!(
        parse(&["ws", "delete", "ws_00"]).command,
        Command::Ws(ws::WsCmd::Delete(_))
    ));

    assert!(matches!(
        parse(&["ref", "list", "release/"]).command,
        Command::Ref(refs::RefCmd::List(_))
    ));
    assert!(matches!(
        parse(&["ref", "show", "trunk"]).command,
        Command::Ref(refs::RefCmd::Show(_))
    ));
    assert!(matches!(
        parse(&["ref", "set", "r", "ab", "--expected", "none"]).command,
        Command::Ref(refs::RefCmd::Set(_))
    ));
    assert!(matches!(
        parse(&["ref", "delete", "r"]).command,
        Command::Ref(refs::RefCmd::Delete(_))
    ));

    assert!(matches!(
        parse(&["admin", "whoami"]).command,
        Command::Admin(admin::AdminCmd::Whoami)
    ));
    match parse(&[
        "admin",
        "token",
        "issue",
        "--repo",
        "m",
        "--verbs",
        "read,write",
        "--paths",
        "src",
        "--expires",
        "1h",
    ])
    .command
    {
        Command::Admin(admin::AdminCmd::Token(admin::TokenCmd::Issue(a))) => {
            assert_eq!(a.verbs, vec!["read", "write"]);
            assert_eq!(a.paths, vec!["src"]);
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        parse(&["admin", "relation", "add", "repo:m", "owner", "principal:p"]).command,
        Command::Admin(admin::AdminCmd::Relation(admin::RelationCmd::Add(_)))
    ));
    assert!(matches!(
        parse(&[
            "admin",
            "relation",
            "remove",
            "repo:m",
            "owner",
            "principal:p"
        ])
        .command,
        Command::Admin(admin::AdminCmd::Relation(admin::RelationCmd::Remove(_)))
    ));
    assert!(matches!(
        parse(&["admin", "relation", "list", "repo:m", "--relation", "owner"]).command,
        Command::Admin(admin::AdminCmd::Relation(admin::RelationCmd::List(_)))
    ));
    assert!(matches!(
        parse(&["admin", "policy", "get", "submit"]).command,
        Command::Admin(admin::AdminCmd::Policy(admin::PolicyCmd::Get(_)))
    ));
    assert!(matches!(
        parse(&["admin", "policy", "set", "submit", "f.cedar"]).command,
        Command::Admin(admin::AdminCmd::Policy(admin::PolicyCmd::Set(_)))
    ));
    assert!(matches!(
        parse(&["admin", "health"]).command,
        Command::Admin(admin::AdminCmd::Health)
    ));
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
        "--storage-dir",
        "/data",
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
