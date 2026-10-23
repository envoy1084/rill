//! Intents, workspaces, refs, access and administration.

use clap::Parser;
use rill_cli::cli::{Cli, Command};
use rill_cli::commands::{access, admin, intent, refs, ws};

use super::parse;

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
        parse(&["op", "log", "ws_00", "-n", "2"]).command,
        Command::Op(ws::OpCmd::Log(_))
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

    assert!(matches!(parse(&["whoami"]).command, Command::Whoami));
    match parse(&[
        "auth",
        "token",
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
        Command::Auth(admin::AuthCmd::Token(a)) => {
            assert_eq!(a.verbs, vec!["read", "write"]);
            assert_eq!(a.paths, vec!["src"]);
        }
        other => panic!("{other:?}"),
    }
    match parse(&["access", "grant", "team:infra", "writer", "src/infra"]).command {
        Command::Access(access::AccessCmd::Grant(a)) => {
            assert_eq!(
                (a.subject.as_str(), a.path.as_str()),
                ("team:infra", "src/infra")
            );
        }
        other => panic!("{other:?}"),
    }
    match parse(&["access", "revoke", "user:ann", "reader"]).command {
        Command::Access(access::AccessCmd::Revoke(a)) => assert_eq!(a.path, ""),
        other => panic!("{other:?}"),
    }
    match parse(&["access", "check", "writer", "src", "--as", "agent:bot"]).command {
        Command::Access(access::AccessCmd::Check(a)) => {
            assert_eq!(a.subject.as_deref(), Some("agent:bot"));
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        parse(&["access", "list", "--relation", "owner"]).command,
        Command::Access(access::AccessCmd::List(_))
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
