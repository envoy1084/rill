//! The `rill` command line: global options and the command tree. The
//! argument structs of each group live next to their implementation in
//! [`crate::commands`].

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::commands::{
    access, admin, change, checkout, history, import, intent, land, org, refs, repo, review, stack,
    status, watch, ws,
};

/// Rill: server-authoritative source control for humans and agents.
#[derive(Parser, Debug)]
#[command(
    name = "rill",
    version,
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

    /// Run as if started in this directory (checkout discovery starts here).
    #[arg(short = 'C', long, global = true, value_name = "DIR")]
    pub cwd: Option<PathBuf>,

    /// More logging on stderr (repeat for more).
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// No logging on stderr.
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Never use colour.
    #[arg(long, global = true)]
    pub no_color: bool,
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

    /// Materialize a repository into a directory and register a workspace.
    Checkout(checkout::CheckoutArgs),

    /// Local changes in the checkout.
    Status(status::StatusArgs),

    /// Differences: local edits, or two points in history.
    Diff(status::DiffArgs),

    /// Walk history.
    Log(history::LogArgs),

    /// Line-by-line attribution of a file.
    Blame(history::BlameArgs),

    /// Show a change version and its files.
    Show(history::ShowArgs),

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

    /// Mark conflicted paths as resolved (or list them).
    Resolve(change::ResolveArgs),

    /// Undo the workspace's last operation and restore its base.
    Undo,

    /// Stacks of changes: show, sync, submit, land.
    #[command(subcommand)]
    Stack(stack::StackCmd),

    /// Submit a change for review.
    Submit(review::SubmitArgs),

    /// Reviews: show, list, vote, comment, threads, evaluate, abandon.
    #[command(subcommand)]
    Review(review::ReviewCmd),

    /// Land a change on trunk.
    Land(land::LandArgs),

    /// Trunk status: head, in-flight landings, recent landings.
    Trunk,

    /// Recent landings with their verification.
    Landings(land::LandingsArgs),

    /// Verification record of a landed sequence.
    Verify(land::VerifyArgs),

    /// Land a revert of a sequence.
    Revert(land::RevertArgs),

    /// Bisect a verification target between two sequences.
    Bisect(land::BisectArgs),

    /// Intents: tasks, bugs and plans that changes link to.
    #[command(subcommand)]
    Intent(intent::IntentCmd),

    /// Server-side workspaces.
    #[command(subcommand)]
    Ws(ws::WsCmd),

    /// Refs: list, show, set, delete.
    #[command(subcommand)]
    Ref(refs::RefCmd),

    /// Sign in through the organization's identity provider.
    Login(crate::commands::session::LoginArgs),

    /// End the stored session for the server.
    Logout,

    /// Identity, tokens, policies and health.
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

    /// Run every role in one process for development: no authentication.
    Dev(crate::server::DevArgs),

    /// Run the service from a configuration file.
    Api(crate::server::ApiArgs),

    /// Run the regional cache tier in front of an origin.
    Cache(crate::server::cache::CacheArgs),
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
