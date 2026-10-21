//! Server configuration: a TOML file with `RILL_*` environment overrides.
//!
//! The file is optional; every section has a default that gives a usable
//! single-node development server (in-memory storage, dev mode, no
//! verification). Environment variables override individual fields so a
//! container can be configured without a file; see [`ServerConfig::apply_env`]
//! for the recognised names.

mod audit;
mod auth;
mod env;
mod identity;
mod server;
mod storage;

pub use audit::AuditConfig;
pub use auth::{AuthConfig, StaticToken};
pub use identity::{IdentityConfig, LifetimesConfig, RotationConfig};
pub use server::{
    LogFormat, PostLandConfig, Roles, SandboxKind, ServerConfig, ShutdownConfig, TelemetryConfig,
    TlsConfig, VerifyConfig, DEFAULT_GRPC_ADDR, DEFAULT_HTTP_ADDR,
};
pub use storage::{MetadataConfig, StorageConfig};
