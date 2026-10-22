//! Stacks, review and landing.

use rill_cli::cli::Command;
use rill_cli::commands::{land, review, stack};

use super::parse;

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
