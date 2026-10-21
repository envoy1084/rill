//! Identity for Rill.
//!
//! * [`keys`]: token signing keys, generated here, wrapped by a key
//!   service (a local master key, Vault Transit, AWS KMS or Google Cloud
//!   KMS), stored in the metadata store and rotated on a schedule.

pub mod error;
pub mod keys;

pub use error::{Error, Result};
