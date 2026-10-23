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
    /// Review a change full-screen: files, diffs, threads; comment, approve,
    /// reject.
    Open(ChangeArg),
    /// Show a review: status, votes, threads, risk.
    Show(ChangeArg),
    /// List reviews.
    List(ListArgs),
    /// Vote on a label.
    Vote(VoteArgs),
    /// Open a thread.
    Comment(CommentArgs),
    /// Reply to a thread.
    Reply(ReplyArgs),
    /// List threads.
    Threads(ChangeArg),
    /// Resolve (or reopen) a thread.
    ResolveThread(ResolveThreadArgs),
    /// Evaluate the submit policy and print the reasons.
    Evaluate(ChangeArg),
    /// Ask reviewer agents to review the current version.
    Request(RequestArgs),
    /// Agent review runs on a change.
    Runs(ChangeArg),
    /// Landings sampled for a human audit, still waiting.
    Audits,
    /// Pass or fail an audit.
    Audit(super::attention::AuditArgs),
    /// Group an intent's changes by the edit they make; approve a group.
    Batch(super::attention::BatchArgs),
    /// What the author changed between two versions, without rebase noise.
    Interdiff(super::mechanics::InterdiffArgs),
    /// Apply a thread's suggested edit as a new version.
    Apply(super::mechanics::ApplyArgs),
    /// Reopen an abandoned change.
    Reopen(ChangeArg),
}

/// `rill review request`.
#[derive(Args, Debug)]
pub struct RequestArgs {
    /// Change id or version hash (default: the checkout's working change).
    pub change: Option<String>,
    /// Reviewer agents to ask (default: as `.rill/review.toml` routes it).
    #[arg(long = "reviewer", value_name = "AGENT")]
    pub reviewers: Vec<String>,
}

/// `rill approve` and `rill reject`.
#[derive(Args, Debug)]
pub struct ApproveArgs {
    /// Change id or version hash (default: the checkout's working change).
    pub change: Option<String>,
    /// The label to vote on.
    #[arg(long, default_value = "Review")]
    pub label: String,
    /// The value (default: the label's most, +2 or -2 for `Review`).
    #[arg(long, allow_hyphen_values = true)]
    pub value: Option<i8>,
    /// Reason for the vote.
    #[arg(short, long)]
    pub message: Option<String>,
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
    /// Through this line, for a range.
    #[arg(long, requires = "line")]
    pub end_line: Option<u32>,
    /// Suggest this text in place of the lines (requires --line).
    #[arg(long, requires = "line")]
    pub suggest: Option<String>,
}

/// `rill review reply`.
#[derive(Args, Debug)]
pub struct ReplyArgs {
    /// Change id or version hash.
    pub change: String,
    /// Thread id.
    pub thread: String,
    /// Reply text.
    #[arg(short, long)]
    pub message: String,
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
