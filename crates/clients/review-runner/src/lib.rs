//! The reference runner for Rill's reviewer fleet.
//!
//! It signs in as one agent (with the SDK's usual credentials: a token, or
//! a workload identity exchanged for one), leases that agent's review or
//! resolve jobs from a repository, runs them on a backend and reports the
//! results. The server checks every result, so a runner is trusted with
//! nothing beyond the agent's own rights.
//!
//! * [`backend`]: the deterministic fake and the Claude Messages API.
//! * [`packet`]: how a review packet is shown to a model, and its added
//!   lines.
//! * [`review`] and [`resolve`]: the lease, run, report loops.

pub mod args;
pub mod backend;
pub mod error;
pub mod packet;
pub mod resolve;
pub mod review;

pub use error::{Error, Result};
