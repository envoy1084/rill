//! The hook through which stored object bytes can be encrypted at rest.

use std::fmt::Debug;

use crate::error::Result;

/// Seals encoded object frames before a persistent backend writes them and
/// opens them after it reads them. A sealed frame is opaque to the store;
/// hashing and verification always happen on the plaintext object.
pub trait Sealer: Send + Sync + Debug {
    fn seal(&self, frame: Vec<u8>) -> Result<Vec<u8>>;
    fn open(&self, stored: Vec<u8>) -> Result<Vec<u8>>;
}

/// Stores frames as they are.
#[derive(Debug, Default, Clone, Copy)]
pub struct Plaintext;

impl Sealer for Plaintext {
    fn seal(&self, frame: Vec<u8>) -> Result<Vec<u8>> {
        Ok(frame)
    }

    fn open(&self, stored: Vec<u8>) -> Result<Vec<u8>> {
        Ok(stored)
    }
}

/// Namespace objects live under until stores are split per organization.
pub const DEFAULT_NAMESPACE: &str = "default";
