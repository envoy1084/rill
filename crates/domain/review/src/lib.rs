//! The review state machine of Rill.
//!
//! A [`ReviewService`] owns no state: every operation reads a
//! [`rill_types::ReviewState`] record, mutates it and writes it back with
//! optimistic concurrency, publishing a [`ReviewEvent`] on success.
//!
//! * [`lifecycle`]: submit, new versions (with Gerrit-style vote copying
//!   across trivial rebases), status transitions.
//! * [`votes`]: labels and vote validation.
//! * [`threads`]: comment threads whose line anchors follow the code.
//! * [`stacks`]: chains of dependent changes.
//! * [`facts`] and [`evaluate`]: building policy facts and deciding
//!   whether a change may land.
//! * [`agents`]: the reviewer fleet's context, result checks and run
//!   records.

pub mod agents;
mod content;
pub mod error;
pub mod evaluate;
pub mod facts;
mod intents;
pub mod lifecycle;
mod owners;
pub mod service;
pub mod stacks;
pub mod threads;
pub mod votes;

#[cfg(test)]
pub(crate) mod testing;

pub use agents::{Budget, Finding, ReviewContext, ReviewResult, ReviewRun, RunKind, RunStatus};
pub use error::{Error, Result};
pub use lifecycle::VersionRelation;
pub use service::{ReviewEvent, ReviewService};
pub use stacks::{Restack, RestackedChange};
pub use threads::{context_hash, CommentMeta};
pub use votes::LabelSet;
