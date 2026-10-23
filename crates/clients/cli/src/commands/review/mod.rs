//! `rill submit`, `rill approve`, `rill reject`, `rill abandon` and
//! `rill review`: show, list, vote, comment, reply, threads, resolve-thread,
//! evaluate, request, runs, audits, audit, batch.

mod args;
pub mod attention;
mod browse;
mod dispatch;
mod fleet;
mod inbox;
mod lifecycle;
mod render;
mod threads;
mod working_change;

pub use args::{
    AbandonArgs, ApproveArgs, ChangeArg, CommentArgs, ListArgs, ResolveThreadArgs, ReviewCmd,
    SubmitArgs, VoteArgs,
};
pub use dispatch::run;
pub use inbox::inbox;
pub use lifecycle::{abandon, approve, submit};
pub use threads::comment;
pub use working_change::{change_or_working, ensure_recorded};
