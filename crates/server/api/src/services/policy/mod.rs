//! `rill.v1.PolicyService`: a repository's submit (Cedar) and risk (JSON)
//! policies, the policy in force, landing explanations, simulation and
//! org guardrails.

mod explain;
mod guardrails;
mod policies;
mod service;

pub use service::PolicySvc;
