//! Command implementations, one module per group. Each module defines its
//! clap argument types and `run` functions that take the shared
//! [`Ctx`](crate::context::Ctx).

pub mod access;
pub mod admin;
pub mod change;
pub mod checkout;
pub mod export;
pub mod history;
pub mod identity;
pub mod import;
pub mod intent;
pub mod land;
pub mod org;
pub mod refs;
pub mod repo;
pub mod review;
pub mod session;
pub mod stack;
pub mod status;
pub mod watch;
pub mod ws;

mod parse;

pub use parse::{change_ref, root_arg, seq_arg, ChangeRef};

use clap::CommandFactory;

use crate::cli::{Cli, Command};
use crate::context::Ctx;
use crate::Result;

/// Dispatch a parsed command.
pub async fn run(ctx: &mut Ctx<'_>, command: Command) -> Result<()> {
    let local = matches!(
        command,
        Command::Dev(_)
            | Command::Api(_)
            | Command::Cache(_)
            | Command::Login(_)
            | Command::Logout
            | Command::Completions(_)
    );
    if !local {
        ctx.resolve_workload().await?;
    }
    match command {
        Command::Repo(cmd) => repo::run(ctx, cmd).await,
        Command::Org(cmd) => org::run(ctx, cmd).await,
        Command::Checkout(args) => checkout::run(ctx, args).await,
        Command::Status(args) => status::status(ctx, args).await,
        Command::Diff(args) => status::diff(ctx, args).await,
        Command::Log(args) => history::log(ctx, args).await,
        Command::Blame(args) => history::blame(ctx, args).await,
        Command::Show(args) => history::show(ctx, args).await,
        Command::Ls(args) => history::ls(ctx, args).await,
        Command::Cat(args) => history::cat(ctx, args).await,
        Command::Grep(args) => history::grep(ctx, args).await,
        Command::New(args) => change::new(ctx, args).await,
        Command::Describe(args) => change::describe(ctx, args).await,
        Command::Amend(args) => change::amend(ctx, args).await,
        Command::Sync(args) => change::sync(ctx, args).await,
        Command::Resolve(args) => change::resolve(ctx, args).await,
        Command::Undo => change::undo(ctx).await,
        Command::Stack(cmd) => stack::run(ctx, cmd).await,
        Command::Submit(args) => review::submit(ctx, args).await,
        Command::Review(cmd) => review::run(ctx, cmd).await,
        Command::Land(args) => land::land(ctx, args).await,
        Command::Trunk => land::trunk(ctx).await,
        Command::Landings(args) => land::landings(ctx, args).await,
        Command::Verify(args) => land::verify(ctx, args).await,
        Command::Revert(args) => land::revert(ctx, args).await,
        Command::Bisect(args) => land::bisect(ctx, args).await,
        Command::Intent(cmd) => intent::run(ctx, cmd).await,
        Command::Ws(cmd) => ws::run(ctx, cmd).await,
        Command::Ref(cmd) => refs::run(ctx, cmd).await,
        Command::Admin(cmd) => admin::run(ctx, cmd).await,
        Command::Access(cmd) => access::run(ctx, cmd).await,
        Command::Login(args) => session::login(ctx, args).await,
        Command::Logout => session::logout(ctx).await,
        Command::Import(cmd) => import::run(ctx, cmd).await,
        Command::Export(cmd) => export::run(ctx, cmd).await,
        Command::Watch(cmd) => watch::run(ctx, cmd).await,
        Command::Dev(args) => crate::server::dev(args).await,
        Command::Api(args) => crate::server::api(ctx, args).await,
        Command::Cache(args) => crate::server::cache::run(args).await,
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
