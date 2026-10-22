//! `rill.v1.ProtectionService`: redactions (requested by one org
//! administrator, applied by another), releasing quarantined versions and
//! rotating data keys.

mod keys;
mod redactions;
mod service;

pub use service::ProtectionSvc;
