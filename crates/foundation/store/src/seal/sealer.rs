//! The hook through which stored object bytes are encrypted at rest.

use std::fmt::Debug;

use async_trait::async_trait;
use rill_types::{ObjectHash, ObjectKind};

use crate::error::Result;

/// The object a frame belongs to; sealing binds the frame to it.
#[derive(Debug, Clone, Copy)]
pub struct ObjectAt<'a> {
    pub namespace: &'a str,
    pub kind: ObjectKind,
    pub hash: &'a ObjectHash,
}

/// Seals encoded object frames before a persistent backend writes them and
/// opens them after it reads them. A sealed frame is opaque to the store;
/// hashing and verification always happen on the plaintext object.
#[async_trait]
pub trait Sealer: Send + Sync + Debug {
    /// Prepare to seal and open `namespace`'s objects; stores call it
    /// before [`Sealer::seal`] or [`Sealer::open`], which cannot wait.
    async fn ready(&self, _namespace: &str) -> Result<()> {
        Ok(())
    }

    fn seal(&self, at: ObjectAt<'_>, frame: Vec<u8>) -> Result<Vec<u8>>;

    fn open(&self, at: ObjectAt<'_>, stored: Vec<u8>) -> Result<Vec<u8>>;
}

/// Stores frames as they are.
#[derive(Debug, Default, Clone, Copy)]
pub struct Plaintext;

#[async_trait]
impl Sealer for Plaintext {
    fn seal(&self, _: ObjectAt<'_>, frame: Vec<u8>) -> Result<Vec<u8>> {
        Ok(frame)
    }

    fn open(&self, _: ObjectAt<'_>, stored: Vec<u8>) -> Result<Vec<u8>> {
        Ok(stored)
    }
}

/// The namespace of the default org's objects; every org has its own.
pub const DEFAULT_NAMESPACE: &str = "default";

/// The org whose data keys seal `namespace`: an org's own namespace, or a
/// namespace beside it such as `acme.exec` (org names have no dots).
pub fn org_of(namespace: &str) -> &str {
    namespace.split('.').next().unwrap_or(namespace)
}
