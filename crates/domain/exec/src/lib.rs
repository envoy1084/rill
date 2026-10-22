//! Execution for Rill.
//!
//! [`remote`] is the REAPI side: each org's execution space (content
//! store and action cache), operations, and the scheduler that queues
//! actions for workers. [`verify`] compiles a tree's `.rill/verify.toml`
//! into actions and runs them through that scheduler. [`worker`] runs
//! actions inside the server with a [`sandbox`], for development and
//! single-host setups; production uses Buildbarn workers.

pub mod error;
pub mod history;
pub mod remote;
pub mod runs;
pub mod sandbox;
pub mod verify;
pub mod worker;

pub use error::{Error, Result};
pub use sandbox::{Invocation, Isolation, LocalSandbox, RawOutcome, Sandbox, TIMEOUT_EXIT_CODE};
pub use verify::{PlanRunner, Target, VerificationOutcome, PLAN_PATH};
pub use worker::LocalWorker;
