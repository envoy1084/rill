//! Token signing keys and the key services that protect them.
//!
//! Biscuit signs with an in-memory key, so a key service cannot sign
//! tokens itself. Instead each P-256 key is generated here, its secret
//! wrapped by the key service and stored with its public half; every
//! server unwraps the key it signs with and verifies with the public keys
//! alone.

#[cfg(feature = "aws")]
mod aws;
mod config;
#[cfg(feature = "gcp")]
mod gcp;
mod local;
mod manager;
mod vault;
mod wrapper;

#[cfg(feature = "aws")]
pub use aws::AwsKms;
pub use config::{key_service, KeyServiceConfig};
#[cfg(feature = "gcp")]
pub use gcp::GcpKms;
pub use local::LocalKeyService;
pub use manager::{KeyManager, KeySet, RotationPolicy};
pub use vault::VaultTransit;
pub use wrapper::KeyWrapper;
