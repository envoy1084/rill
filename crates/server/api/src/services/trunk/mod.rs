//! `rill.v1.TrunkService`: trunk status, verification, incidents, reverts
//! and bisection.

mod health;
mod reverts;
mod service;
mod status;
mod verification;

pub use service::TrunkSvc;
