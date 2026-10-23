//! `rill submit`, `rill approve`, `rill reject`, `rill abandon` and
//! `rill review`: show, list, vote, comment, threads, resolve-thread,
//! evaluate.

mod args;
mod dispatch;
mod lifecycle;
mod render;
mod threads;
mod working_change;

pub use args::{
    AbandonArgs, ApproveArgs, ChangeArg, CommentArgs, ListArgs, ResolveThreadArgs, ReviewCmd,
    SubmitArgs, VoteArgs,
};
pub use dispatch::run;
pub use lifecycle::{abandon, approve, submit};
pub use working_change::change_or_working;
