//! Agents: exchanging a workload's credential for the agent it is, and
//! whether an agent may act now.

mod exchange;
mod standing;

pub use exchange::{exchange_workload, WorkloadKeys};
pub use standing::{agent_may_act, registration_holds, sponsor_is_active};
