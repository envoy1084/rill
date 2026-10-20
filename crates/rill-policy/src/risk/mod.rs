//! Deterministic, explainable risk classification.
//!
//! [`classify`] maps [`ChangeFacts`] plus a [`RiskConfig`] to a
//! [`RiskAssessment`]: a [`rill_types::RiskClass`] and the list of features
//! that produced it.

mod classify;
mod config;
mod facts;

pub use classify::{classify, touches_sensitive, RiskAssessment, SENSITIVE_REASON_PREFIX};
pub use config::{CompiledRiskConfig, RiskConfig, SensitiveGlob};
pub use facts::{ChangeFacts, PathFacts};
