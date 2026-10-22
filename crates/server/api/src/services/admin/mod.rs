//! `rill.v1.AdminService`: the server's health, and importing existing
//! history onto trunk.

mod health;
mod service;
mod trunk;

pub use health::health_components;
pub use service::AdminSvc;
