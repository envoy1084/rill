//! Storage for Rill.
//!
//! Two stores with different access patterns:
//!
//! * [`ObjectStore`]: immutable, content-addressed bytes (blobs, chunks,
//!   trees, change versions, derived data). Any cache can serve it.
//! * [`MetaStore`]: a small, ordered, transactional key-value space for
//!   everything mutable or ordered: refs, the change log, review and intent
//!   records, workspace state.
//!
//! [`RepoStore`] layers the typed keyspace of a repository over the two
//! traits so services never touch raw keys.
//!
//! Backends:
//!
//! * [`memory`]: reference semantics, tests and single-process servers.
//! * [`local`] (feature `local`, default): objects as zstd-framed files,
//!   metadata in a redb database. One directory, no services.
//! * [`s3`] (feature `s3`): objects on S3-compatible storage via
//!   `object_store`.
//! * [`fdb`] (feature `fdb`): metadata in FoundationDB.

pub mod codec;
pub mod error;
pub mod keys;
pub mod keyspace;
pub mod memory;
pub mod meta;
pub mod object;
pub mod repo;
pub mod seal;
pub mod verified;

#[cfg(feature = "local")]
pub mod local;

#[cfg(feature = "s3")]
pub mod s3;

#[cfg(feature = "fdb")]
pub mod fdb;

pub use error::{Error, Result};
#[cfg(feature = "fdb")]
pub use fdb::FdbMetaStore;
pub use keyspace::Keyspace;
#[cfg(feature = "local")]
pub use local::{LocalObjectStore, LocalOptions, LocalStores, RedbMetaStore};
pub use memory::{MemoryMetaStore, MemoryObjectStore};
pub use meta::{Condition, KeyRange, MetaStore, Mutation, Transaction, TxOutcome};
pub use object::{ObjectStore, ObjectStoreExt, StoredObject};
pub use repo::{OrgConfig, RepoSettings, RepoStore, Stores, DEFAULT_ORG, STORE_FORMAT};
#[cfg(feature = "s3")]
pub use s3::ObjectStoreBackend;
pub use seal::{Plaintext, Sealer, DEFAULT_NAMESPACE};
pub use verified::VerifiedObjectStore;
