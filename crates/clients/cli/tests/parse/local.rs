//! Repository and checkout-local commands.

use clap::Parser;
use rill_cli::cli::{Cli, Command};
use rill_cli::commands::repo;

use super::parse;

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
        parse(&["repo", "show"]).command,
        Command::Repo(repo::RepoCmd::Show)
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
    assert!(matches!(parse(&["conflicts"]).command, Command::Conflicts));
    assert!(
        matches!(parse(&["resolve", "a", "b"]).command, Command::Resolve(a) if a.paths.len() == 2)
    );
    assert!(matches!(parse(&["undo"]).command, Command::Undo));
    assert!(
        Cli::try_parse_from(["rill", "new"]).is_err(),
        "new requires -m"
    );
}
