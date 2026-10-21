//! Verification runner for Rill.
//!
//! An [`Action`] names a command, its environment and the input tree it
//! runs against; its digest is the key into a content-addressed
//! [`ActionCache`] stored in the repository. A [`Runner`] materializes the
//! input tree, executes the command in a [`Sandbox`] and stores the
//! result, so an action with a known digest is never run twice.
//!
//! [`verify_tree`] ties this to repositories: a tree's `.rill/verify.toml`
//! declares the steps that verify it, and the outcome is the conjunction
//! of their results.

pub mod action;
pub mod cache;
pub mod error;
pub mod materialize;
pub mod runner;
pub mod sandbox;
pub mod verify;

pub use action::{host_platform, Action, ActionResult, TIMEOUT_EXIT_CODE};
pub use cache::{ActionCache, CACHE_KIND};
pub use error::{Error, Result};
pub use materialize::{materialize, MaterializeOptions, MaterializedFile};
pub use runner::{CacheStatus, Runner};
pub use sandbox::{Isolation, LocalSandbox, RawOutcome, Sandbox};
pub use verify::{
    load_plan, verify_plan, verify_tree, PlanStep, StepResult, VerificationOutcome,
    VerificationPlan, PLAN_PATH,
};
