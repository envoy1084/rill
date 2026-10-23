//! Routing a parsed command to its group's implementation.

use clap::CommandFactory;

use super::{
    access, admin, blame, bug_report, change, checkout, clone, daemon, diff, doctor, export,
    health, help, history, identity, import, intent, land, log, man, org, refs, release, repo,
    review, session, show, stack, status, watch, ws,
};
use crate::cli::{Cli, Command};
use crate::context::Ctx;
use crate::Result;

/// Dispatch a parsed command.
pub async fn run(ctx: &mut Ctx<'_>, command: Command) -> Result<()> {
    let local = matches!(
        command,
        Command::Dev(_)
            | Command::Login(_)
            | Command::Logout
            | Command::Completions(_)
            | Command::Daemon(_)
            | Command::Doctor
            | Command::Help(_)
            | Command::Man(_)
    );
    if !local {
        ctx.resolve_workload().await?;
    }
    match command {
        Command::Repo(cmd) => repo::run(ctx, cmd).await,
        Command::Org(cmd) => org::run(ctx, cmd).await,
        Command::Clone(args) => clone::run(ctx, args).await,
        Command::Checkout(args) => checkout::run(ctx, args).await,
        Command::Commit(args) => change::commit_cmd(ctx, args).await,
        Command::Redo => change::redo(ctx).await,
        Command::Squash(args) => change::squash(ctx, args).await,
        Command::Split(args) => change::split(ctx, args).await,
        Command::Inbox => review::inbox(ctx).await,
        Command::Comment(args) => review::comment(ctx, args).await,
        Command::BugReport(args) => bug_report::run(ctx, args).await,
        Command::Help(args) => help::run(ctx, args).await,
        Command::Man(args) => man::run(ctx, args).await,
        Command::Daemon(cmd) => daemon::run(ctx, cmd).await,
        Command::Doctor => doctor::run(ctx).await,
        Command::Status(args) => status::status(ctx, args).await,
        Command::Diff(args) => diff::diff(ctx, args).await,
        Command::Log(args) => log::log(ctx, args).await,
        Command::Blame(args) => blame::blame(ctx, args).await,
        Command::Show(args) => show::show(ctx, args).await,
        Command::Ls(args) => history::ls(ctx, args).await,
        Command::Cat(args) => history::cat(ctx, args).await,
        Command::Grep(args) => history::grep(ctx, args).await,
        Command::New(args) => change::new(ctx, args).await,
        Command::Describe(args) => change::describe(ctx, args).await,
        Command::Amend(args) => change::amend(ctx, args).await,
        Command::Sync(args) => change::sync(ctx, args).await,
        Command::Conflicts => change::conflicts(ctx).await,
        Command::Resolve(args) => change::resolve(ctx, args).await,
        Command::Undo => change::undo(ctx).await,
        Command::Op(cmd) => ws::op(ctx, cmd).await,
        Command::Abandon(args) => review::abandon(ctx, args).await,
        Command::Approve(args) => review::approve(ctx, args, false).await,
        Command::Reject(args) => review::approve(ctx, args, true).await,
        Command::Stack(cmd) => stack::run(ctx, cmd).await,
        Command::Submit(args) => review::submit(ctx, args).await,
        Command::Review(cmd) => review::run(ctx, cmd).await,
        Command::Land(args) => land::land(ctx, args).await,
        Command::Queue => land::queue(ctx).await,
        Command::Trunk(args) => match args.cmd {
            None => land::trunk(ctx).await,
            Some(cmd) => health::run(ctx, cmd).await,
        },
        Command::Incidents(args) => health::incidents(ctx, args).await,
        Command::Revert(args) => land::revert(ctx, args).await,
        Command::Bisect(args) => land::bisect(ctx, args).await,
        Command::Checks(cmd) => super::checks::run(ctx, cmd).await,
        Command::Intent(cmd) => intent::run(ctx, cmd).await,
        Command::Ws(cmd) => ws::run(ctx, cmd).await,
        Command::Ref(cmd) => refs::run(ctx, cmd).await,
        Command::Release(cmd) => release::run(ctx, cmd).await,
        Command::Owners(cmd) => crate::commands::owners::run(ctx, cmd).await,
        Command::Notify(cmd) => crate::commands::notify::run(ctx, cmd).await,
        Command::Policy(cmd) => crate::commands::policy::run(ctx, cmd).await,
        Command::Why(args) => crate::commands::policy::why(ctx, args).await,
        Command::Reviewer(cmd) => crate::commands::reviewer::run(ctx, cmd).await,
        Command::Admin(cmd) => admin::run(ctx, cmd).await,
        Command::Access(cmd) => access::run(ctx, cmd).await,
        Command::Login(args) => session::login(ctx, args).await,
        Command::Logout => session::logout(ctx).await,
        Command::Whoami => admin::whoami(ctx).await,
        Command::Auth(cmd) => admin::auth(ctx, cmd).await,
        Command::Agent(cmd) => identity::agents::agent(ctx, cmd).await,
        Command::Import(cmd) => import::run(ctx, cmd).await,
        Command::Export(cmd) => export::run(ctx, cmd).await,
        Command::Watch(cmd) => watch::run(ctx, cmd).await,
        Command::Dev(args) => crate::commands::dev::run(args),
        Command::Completions(args) => {
            let mut cmd = Cli::command();
            let mut buf = Vec::new();
            clap_complete::generate(args.shell, &mut cmd, "rill", &mut buf);
            ctx.out().raw(&buf)?;
            ctx.out().flush()?;
            Ok(())
        }
    }
}
