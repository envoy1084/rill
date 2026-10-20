//! Commit-order positions assigned by the metadata store.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// A versionstamp: 10 bytes of commit order assigned by the store, then 2
/// bytes of order within the transaction. Byte order is commit order, so a
/// stamp is a resumable cursor into any log written with stamps.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Versionstamp(pub [u8; 12]);

impl Versionstamp {
    /// Parse 12 raw bytes.
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        bytes
            .try_into()
            .map(Versionstamp)
            .map_err(|_| Error::InvalidId(format!("versionstamp of {} bytes", bytes.len())))
    }

    /// The raw bytes.
    pub fn as_bytes(&self) -> &[u8; 12] {
        &self.0
    }
}

impl fmt::Display for Versionstamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&hex::encode(self.0))
    }
}

impl fmt::Debug for Versionstamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Versionstamp({self})")
    }
}

impl FromStr for Versionstamp {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let bytes = hex::decode(s).map_err(|e| Error::InvalidId(format!("{s:?}: {e}")))?;
        Versionstamp::from_slice(&bytes)
    }
}
