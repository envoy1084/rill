//! Client library for Rill.
//!
//! [`Client`] holds one connection to a server and hands out typed,
//! repository-scoped clients: [`RepoClient`] for history, trees, blobs and
//! writes, plus [`WorkspaceClient`], [`ReviewClient`], [`IntentClient`] and
//! [`LanderClient`] for the other services and [`AdminClient`] for the
//! instance. Inputs and outputs are `rill-types` values where one exists
//! and the small structs in [`types`] otherwise.
//!
//! [`stack`] composes those calls into stack-level operations and
//! [`local`] provides a minimal working copy for the CLI.
//!
//! ```no_run
//! # async fn demo() -> rill_sdk::Result<()> {
//! use rill_sdk::{Client, ClientConfig, LogStart};
//!
//! let client = Client::connect(ClientConfig::new("http://localhost:7000")).await?;
//! let repo = client.repo("main");
//! for entry in repo.log(LogStart::Trunk, 10, None).await? {
//!     println!("{} {}", entry.hash.short(), entry.summary());
//! }
//! # Ok(()) }
//! ```

#![warn(missing_docs)]

pub mod access;
pub mod admin;
pub mod audit;
pub mod auth;
pub mod client;
pub mod config;
pub mod error;
mod files;
pub mod grants;
pub mod intent;
pub mod lander;
pub mod local;
pub mod repo;
pub mod review;
pub mod stack;
pub mod stream;
pub mod types;
pub mod workspace;

pub use access::AccessClient;
pub use admin::AdminClient;
pub use audit::AuditClient;
pub use client::Client;
pub use config::{ClientConfig, SessionConfig, TlsOptions};
pub use connectrpc::ErrorCode as Code;
pub use error::{Error, ErrorReason, Result};
pub use intent::IntentClient;
pub use lander::LanderClient;
pub use local::{Checkout, LocalChange, LocalChangeKind};
pub use repo::{RepoClient, WatchStart};
pub use review::ReviewClient;
pub use stack::{Backoff, StackClient};
pub use stream::EventStream;
pub use types::*;
pub use workspace::WorkspaceClient;

pub use rill_types;
