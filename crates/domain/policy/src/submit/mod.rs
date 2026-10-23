//! Cedar-based submit rules.
//!
//! A [`SubmitPolicy`] is a validated Cedar policy set evaluated against a
//! `Rill::Principal` (who wants to land), a `Rill::Change` (the change's
//! risk, labels and threads) and the `Rill::Action::"land"` action. See
//! [`SCHEMA_SRC`] for the entity model and [`DEFAULT_POLICY`] for the rules a
//! repository starts with.

mod default_policy;
mod entities;
mod facts;
mod policy;
mod schema;
#[cfg(test)]
mod tests;

pub use default_policy::{default_policy, DEFAULT_POLICY};
pub use facts::{owner_coverage, SubmitFacts, VoteSummary, OWNER_LABEL};
pub use policy::{SubmitDecision, SubmitPolicy};
pub use schema::{schema, SCHEMA_SRC};
