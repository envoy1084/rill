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
//! * [`backend`]: objects through `object_store`, as zstd-framed files on
//!   the local filesystem or (feature `s3`) on S3-compatible storage.
//! * [`local`] (feature `local`, default): local objects plus metadata in
//!   a redb database. One directory, no services.
//! * [`fdb`] (feature `fdb`): metadata in FoundationDB.

pub mod audit;
pub mod codec;
pub mod error;
pub mod identity;
pub mod keys;
pub mod keyspace;
pub mod memory;
pub mod meta;
pub mod object;
pub mod redact;
pub mod repo;
pub mod seal;
pub mod verified;

#[cfg(feature = "local")]
pub mod local;

pub mod backend;

#[cfg(feature = "fdb")]
pub mod fdb;

pub use backend::ObjectStoreBackend;
pub use error::{Error, Result};
#[cfg(feature = "fdb")]
pub use fdb::FdbMetaStore;
pub use keyspace::Keyspace;
#[cfg(feature = "local")]
pub use local::{LocalStores, RedbMetaStore};
pub use memory::{MemoryMetaStore, MemoryObjectStore};
pub use meta::{Condition, KeyRange, MetaStore, Mutation, Transaction, TxOutcome};
pub use object::{ObjectStore, ObjectStoreExt, StoredObject};
pub use repo::{OrgConfig, RepoSettings, RepoStore, Stores, DEFAULT_ORG, STORE_FORMAT};
pub use seal::{
    DataKey, DataKeySource, ObjectAt, OrgSealer, Plaintext, Sealer, DEFAULT_NAMESPACE, KEY_REFRESH,
};
pub use verified::VerifiedObjectStore;
