//! Command implementations, one module per group. Each module defines its
//! clap argument types and `run` functions that take the shared
//! [`Ctx`](crate::context::Ctx).

pub mod access;
pub mod admin;
pub mod audit;
pub mod blame;
pub mod bug_report;
pub mod change;
pub mod checkout;
pub mod checks;
pub mod clone;
pub mod daemon;
pub mod dev;
pub mod diff;
pub mod doctor;
pub mod export;
pub mod health;
pub mod help;
pub mod history;
pub mod identity;
pub mod import;
pub mod intent;
pub mod land;
pub mod log;
pub mod man;
pub mod org;
pub mod protection;
pub mod refs;
pub mod release;
pub mod repo;
pub mod review;
pub mod secrets;
pub mod session;
pub mod show;
pub mod stack;
pub mod status;
pub mod watch;
pub mod ws;

mod dispatch;
mod parse;

pub use dispatch::run;
pub use parse::{change_ref, root_arg, seq_arg, ChangeRef};
