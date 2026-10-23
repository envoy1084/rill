//! Client library for Rill.
//!
//! [`Client`] holds one connection to a server and hands out typed,
//! repository-scoped clients: [`RepoClient`] for history, trees, blobs and
//! writes, plus [`WorkspaceClient`], [`ReviewClient`], [`IntentClient`] and
//! [`TrunkClient`] for the other services and [`AdminClient`] for the
//! instance. Inputs and outputs are `rill-types` values where one exists
//! and the small structs in [`types`] otherwise.
//!
//! [`stack`] composes those calls into stack-level operations; working
//! copies are the client daemon's (`rill-daemon`).
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
mod cache_tier;
pub mod checks;
pub mod client;
pub mod config;
pub mod error;
mod files;
pub mod filter;
pub mod grants;
pub mod identity;
pub mod intent;
pub mod names;
pub mod operations;
mod paging;
pub mod protection;
pub mod read_stats;
pub mod repo;
pub mod review;
pub mod secrets;
pub mod stack;
pub mod stream;
#[cfg(feature = "testing")]
pub mod testing;
pub mod trunk;
pub mod types;
pub mod workspace;

pub use access::AccessClient;
pub use admin::AdminClient;
pub use audit::AuditClient;
pub use checks::ChecksClient;
pub use client::Client;
pub use config::{ClientConfig, SessionConfig, TlsOptions};
pub use connectrpc::ErrorCode as Code;
pub use error::{Error, ErrorReason, Result};
pub use identity::IdentityClient;
pub use intent::IntentClient;
pub use operations::OperationClient;
pub use protection::ProtectionClient;
pub use repo::{RepoClient, WatchStart};
pub use review::ReviewClient;
pub use secrets::SecretClient;
pub use stack::{Backoff, StackClient};
pub use stream::EventStream;
pub use trunk::TrunkClient;
pub use types::*;
pub use workspace::WorkspaceClient;

/// The generated `rill.v1` messages, for calls the typed clients do not
/// wrap and for the records some return as is.
pub use rill_proto::rill::v1 as pb;
pub use rill_types;
