//! Server configuration: a TOML file with `RILL_*` environment overrides.
//!
//! The file is optional; every section has a default that gives a usable
//! single-node development server (in-memory storage, dev mode, no
//! verification). Environment variables override individual fields so a
//! container can be configured without a file; see [`ServerConfig::apply_env`]
//! for the recognised names.

mod audit;
mod auth;
mod encryption;
mod env;
mod exec;
mod identity;
mod notify;
mod rate_limit;
mod server;
mod storage;

pub use audit::AuditConfig;
pub use auth::{AuthConfig, StaticToken};
pub use encryption::EncryptionConfig;
pub use exec::{ExecConfig, LocalWorkersConfig, SandboxKind};
pub use identity::{IdentityConfig, LifetimesConfig, RotationConfig};
pub use notify::NotifyConfig;
pub use rate_limit::RateLimitConfig;
pub use server::{
    FlakeConfig, LogFormat, PostLandConfig, Roles, ServerConfig, ShutdownConfig, TelemetryConfig,
    TlsConfig, VerifyConfig, DEFAULT_GRPC_ADDR, DEFAULT_HTTP_ADDR,
};
pub use storage::{MetadataConfig, StorageConfig};
