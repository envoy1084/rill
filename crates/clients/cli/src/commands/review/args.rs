//! Arguments of `rill submit` and `rill review`.

use clap::{Args, Subcommand};

/// `rill submit`.
#[derive(Args, Debug)]
pub struct SubmitArgs {
    /// Change to submit (default: the checkout's working change).
    pub change: Option<String>,
}

/// `rill review <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum ReviewCmd {
    /// Show a review: status, votes, threads, risk.
    Show(ChangeArg),
    /// List reviews.
    List(ListArgs),
    /// Vote on a label.
    Vote(VoteArgs),
    /// Open a thread.
    Comment(CommentArgs),
    /// List threads.
    Threads(ChangeArg),
    /// Resolve (or reopen) a thread.
    ResolveThread(ResolveThreadArgs),
    /// Evaluate the submit policy and print the reasons.
    Evaluate(ChangeArg),
    /// Abandon a change.
    Abandon(AbandonArgs),
}

/// A command taking one change.
#[derive(Args, Debug)]
pub struct ChangeArg {
    /// Change id or version hash (default: the checkout's working change).
    pub change: Option<String>,
}

/// `rill review list`.
#[derive(Args, Debug)]
pub struct ListArgs {
    /// Only this status (open, landable, landing, landed, abandoned).
    #[arg(long)]
    pub status: Option<String>,
    /// Only reviews owned by this principal, as shown (`ann`, `agent:bot`).
    #[arg(long)]
    pub owner: Option<String>,
    /// Page size.
    #[arg(long, default_value_t = 50)]
    pub limit: u32,
}

/// `rill review vote`.
#[derive(Args, Debug)]
pub struct VoteArgs {
    /// Change id or version hash.
    pub change: String,
    /// Label name (Review, Verified, Owner, ...).
    pub label: String,
    /// Vote value, e.g. +2 or -1.
    #[arg(allow_hyphen_values = true)]
    pub value: String,
    /// Reason for the vote.
    #[arg(short, long)]
    pub message: Option<String>,
}

/// `rill review comment`.
#[derive(Args, Debug)]
pub struct CommentArgs {
    /// Change id or version hash.
    pub change: String,
    /// Comment text.
    #[arg(short, long)]
    pub message: String,
    /// Anchor to this file.
    #[arg(long)]
    pub path: Option<String>,
    /// Anchor to this line (requires --path).
    #[arg(long)]
    pub line: Option<u32>,
}

/// `rill review resolve-thread`.
#[derive(Args, Debug)]
pub struct ResolveThreadArgs {
    /// Change id or version hash.
    pub change: String,
    /// Thread id.
    pub thread: String,
    /// Reopen instead of resolving.
    #[arg(long)]
    pub reopen: bool,
}

/// `rill review abandon`.
#[derive(Args, Debug)]
pub struct AbandonArgs {
    /// Change id or version hash.
    pub change: String,
    /// Why.
    #[arg(short, long)]
    pub message: Option<String>,
}
