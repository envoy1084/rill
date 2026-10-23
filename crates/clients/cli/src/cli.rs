//! The `rill` command line: global options and the command tree. The
//! argument structs of each group live next to their implementation in
//! [`crate::commands`].

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::commands::{
    access, admin, blame, bug_report, change, checkout, clone, daemon, diff, health, help, history,
    identity, import, intent, land, log, man, org, refs, release, repo, review, show, stack,
    status, watch, ws,
};

/// Rill: server-authoritative source control for humans and agents.
#[derive(Parser, Debug)]
#[command(
    name = "rill",
    version,
    disable_help_subcommand = true,
    about = "Rill: server-authoritative source control for humans and agents",
    propagate_version = true
)]
pub struct Cli {
    /// Options every command accepts.
    #[command(flatten)]
    pub global: GlobalOpts,

    /// The command to run.
    #[command(subcommand)]
    pub command: Command,
}

/// Options every command accepts.
#[derive(Args, Debug, Clone, Default)]
pub struct GlobalOpts {
    /// Server endpoint, e.g. http://localhost:7420 [default: the checkout's,
    /// then $RILL_ENDPOINT, then ~/.config/rill/config.toml].
    #[arg(long, global = true, value_name = "URL")]
    pub endpoint: Option<String>,

    /// Bearer token [default: $RILL_TOKEN, then the config file].
    #[arg(long, global = true, value_name = "TOKEN")]
    pub token: Option<String>,

    /// Repository name or id [default: the checkout's, then $RILL_REPO].
    #[arg(long, global = true, value_name = "REPO")]
    pub repo: Option<String>,

    /// Print the result as JSON (stable machine output) instead of tables.
    #[arg(long, global = true)]
    pub json: bool,

    /// Print JSON in this schema version (only `v1` exists); implies
    /// `--json`.
    #[arg(long, global = true, value_name = "VERSION", value_parser = ["v1"])]
    pub json_schema: Option<String>,

    /// The client daemon's directory [default: the checkout's daemon, then
    /// $RILL_DAEMON_DIR, then the per-user data directory].
    #[arg(long, global = true, value_name = "DIR")]
    pub daemon_dir: Option<PathBuf>,

    /// Run as if started in this directory (checkout discovery starts here).
    #[arg(short = 'C', long, global = true, value_name = "DIR")]
    pub cwd: Option<PathBuf>,

    /// More logging on stderr (repeat for more).
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// No logging on stderr.
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Never use colour (same as `--color never`).
    #[arg(long, global = true)]
    pub no_color: bool,

    /// When to colour: `auto` (a terminal), `always` or `never`
    /// [default: `[color] ui`, else auto].
    #[arg(long, global = true, value_name = "WHEN", value_parser = ["auto", "always", "never"])]
    pub color: Option<String>,

    /// Print long output directly instead of through the pager.
    #[arg(long, global = true)]
    pub no_pager: bool,
}

/// Top-level commands.
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Repositories: list, create, inspect.
    #[command(subcommand)]
    Repo(repo::RepoCmd),

    /// Organizations: list, create.
    #[command(subcommand)]
    Org(org::OrgCmd),

    /// Clone a repository into a directory named after it (lazily where
    /// the platform allows).
    Clone(clone::CloneArgs),

    /// Check out a repository into a directory, served by the daemon.
    Checkout(checkout::CheckoutArgs),

    /// The client daemon: run, start, stop, status.
    #[command(subcommand)]
    Daemon(daemon::DaemonCmd),

    /// How this host serves checkouts: server, daemon, cache tiers, warnings.
    Doctor,

    /// Local changes in the checkout.
    Status(status::StatusArgs),

    /// Differences: local edits, or two points in history.
    Diff(diff::DiffArgs),

    /// Walk history.
    Log(log::LogArgs),

    /// Line-by-line attribution of a file.
    Blame(blame::BlameArgs),

    /// Show a change version and its files.
    Show(show::ShowArgs),

    /// List a directory at a root.
    Ls(history::LsArgs),

    /// Print a file at a root.
    Cat(history::CatArgs),

    /// Search file contents at a root.
    Grep(history::GrepArgs),

    /// Snapshot the checkout into a new change.
    New(change::NewArgs),

    /// Change the working change's message.
    Describe(change::DescribeArgs),

    /// Snapshot the checkout into the existing working change.
    Amend(change::AmendArgs),

    /// Update the checkout to a new base, merging local edits.
    Sync(change::SyncArgs),

    /// The checkout's conflicted paths.
    Conflicts,

    /// Mark conflicted paths as resolved.
    Resolve(change::ResolveArgs),

    /// Record the working copy as the next version of the working change.
    Commit(change::CommitArgs),

    /// Undo the workspace's last operation and restore its base.
    Undo,

    /// Redo what the last undo undid.
    Redo,

    /// Fold the working change into the unlanded change under it.
    Squash(change::SquashArgs),

    /// Move some paths' edits into a new change under the working change.
    Split(change::SplitArgs),

    /// Reviews waiting on you, and the state of your own.
    Inbox,

    /// Comment on a change (a new review thread).
    Comment(review::CommentArgs),

    /// Write a bug report file: versions, doctor, settings, daemon log.
    BugReport(bug_report::BugReportArgs),

    /// Guides (`rill help stacks`) or a command's help (`rill help land`).
    Help(help::HelpArgs),

    /// Write man pages for every command.
    #[command(hide = true)]
    Man(man::ManArgs),

    /// The workspace's operation log.
    #[command(subcommand)]
    Op(ws::OpCmd),

    /// Abandon a change.
    Abandon(review::AbandonArgs),

    /// Stacks of changes: show, sync, submit, land.
    #[command(subcommand)]
    Stack(stack::StackCmd),

    /// Submit a change for review.
    Submit(review::SubmitArgs),

    /// Approve a change: vote `Review +2` (or `--label`).
    Approve(review::ApproveArgs),

    /// Reject a change: vote `Review -2` (or `--label`).
    Reject(review::ApproveArgs),

    /// Reviews: show, list, vote, comment, threads, evaluate.
    #[command(subcommand)]
    Review(review::ReviewCmd),

    /// Land a change on trunk.
    Land(land::LandArgs),

    /// Land requests waiting, in serve order, with their place and wait.
    Queue,

    /// Trunk: status, health, landings and their verification, freeze.
    Trunk(health::TrunkArgs),

    /// Trunk's incidents, open and closed, newest first.
    Incidents(health::IncidentsArgs),

    /// Land a revert of a sequence.
    Revert(land::RevertArgs),

    /// Bisect a verification target between two sequences.
    Bisect(land::BisectArgs),

    /// Test history and flaky tests from verification.
    #[command(subcommand)]
    Checks(crate::commands::checks::ChecksCmd),

    /// Intents: tasks, bugs and plans that changes link to.
    #[command(subcommand)]
    Intent(intent::IntentCmd),

    /// Server-side workspaces.
    #[command(subcommand)]
    Ws(ws::WsCmd),

    /// Refs: list, show, set, delete.
    #[command(subcommand)]
    Ref(refs::RefCmd),

    /// Releases: cut and list.
    #[command(subcommand)]
    Release(release::ReleaseCmd),

    /// Sign in through the organization's identity provider.
    Login(crate::commands::session::LoginArgs),

    /// End the stored session for the server.
    Logout,

    /// Who the server thinks you are.
    Whoami,

    /// Credentials for scripts: print a capability token.
    #[command(subcommand)]
    Auth(admin::AuthCmd),

    /// Registered agents: register, list, suspend, activate.
    #[command(subcommand)]
    Agent(identity::AgentCmd),

    /// Org administration: policies, health, identity providers, users,
    /// keys, trust, audit, protection, secrets, SCIM.
    #[command(subcommand)]
    Admin(admin::AdminCmd),

    /// Who may read, review, write or own what in a repository.
    #[command(subcommand)]
    Access(access::AccessCmd),

    /// Import history from another system (git).
    #[command(subcommand)]
    Import(import::ImportCmd),

    /// Mirror trunk into another system (git), one way.
    #[command(subcommand)]
    Export(crate::commands::export::ExportCmd),

    /// Stream trunk, ref or log events.
    #[command(subcommand)]
    Watch(watch::WatchCmd),

    /// Print a shell completion script.
    Completions(CompletionsArgs),

    /// Run a development server (every role, no authentication) with
    /// `rill-server dev`.
    Dev(crate::commands::dev::DevArgs),
}

/// `rill completions`.
#[derive(Args, Debug)]
pub struct CompletionsArgs {
    /// The shell to generate for.
    pub shell: clap_complete::Shell,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn command_tree_is_consistent() {
        Cli::command().debug_assert();
    }
}
