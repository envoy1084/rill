//! Encryption of stored objects at rest.

use serde::{Deserialize, Serialize};

/// `[encryption]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EncryptionConfig {
    /// Seal every org's objects under its own data keys. Unset means on,
    /// except in dev mode. In-memory storage is never sealed.
    pub enabled: Option<bool>,
}

impl EncryptionConfig {
    /// Whether objects are sealed on a server in `dev_mode`.
    pub fn enabled(&self, dev_mode: bool) -> bool {
        self.enabled.unwrap_or(!dev_mode)
    }
}
