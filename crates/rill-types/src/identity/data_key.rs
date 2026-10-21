//! Data keys: each org's objects are encrypted at rest under keys only
//! that org's objects use, wrapped by the instance's key service.

use serde::{Deserialize, Serialize};

use super::keys::WrappedSecret;
use crate::encoding::Record;
use crate::time::Timestamp;

/// One data key of an org. Every key stays to open what it sealed; new
/// objects are sealed with the newest active one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataKeyRecord {
    pub org: String,
    /// Counts up from 1 within the org.
    pub id: u32,
    /// 32 random bytes, wrapped.
    pub secret: WrappedSecret,
    pub created_at: Timestamp,
    /// Seals only from here on, so every server holds the key before any
    /// object needs it.
    pub active_from: Timestamp,
}

impl Record for DataKeyRecord {
    const TAG: &'static str = "datakey";
}
