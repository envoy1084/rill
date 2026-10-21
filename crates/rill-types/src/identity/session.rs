//! Sign-in sessions and agent runs.

use serde::{Deserialize, Serialize};

use crate::encoding::Record;
use crate::ids::SessionId;
use crate::principal::Principal;
use crate::time::Timestamp;

/// How a session started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    /// A browser sign-in through the identity provider.
    Browser,
    /// A CLI sign-in through the device flow.
    Device,
    /// An agent or CI run that exchanged a workload credential.
    Workload,
    /// Tokens minted by an administrator.
    Admin,
}

/// A session: the tokens issued under it die with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: SessionId,
    pub org: String,
    pub principal: Principal,
    pub kind: SessionKind,
    pub created_at: Timestamp,
    /// No token is issued under the session after this.
    pub expires_at: Timestamp,
    /// BLAKE3 of the refresh secret; `None` when the session cannot refresh.
    pub refresh_hash: Option<[u8; 32]>,
    pub revoked_at: Option<Timestamp>,
    /// The sponsor and model an agent run declared.
    pub sponsor: Option<String>,
    pub model: Option<String>,
}

impl Record for SessionRecord {
    const TAG: &'static str = "session";
}

impl SessionRecord {
    /// Whether tokens may be issued or honoured under the session at `now`.
    pub fn is_live_at(&self, now: Timestamp) -> bool {
        self.revoked_at.is_none() && now < self.expires_at
    }
}
