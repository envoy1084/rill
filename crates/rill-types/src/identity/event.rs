//! The identity change log: what every server's identity view follows.

use serde::{Deserialize, Serialize};

use crate::encoding::Record;
use crate::ids::{SessionId, TokenId};
use crate::principal::Principal;
use crate::time::Timestamp;

/// What changed. Entries name the record; followers reread it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdentityChange {
    /// A token signing key was added, activated or retired.
    KeysChanged,
    TokenRevoked {
        token_id: TokenId,
        expires_at: Timestamp,
    },
    SessionRevoked {
        session: SessionId,
    },
}

/// One entry of the identity change log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityEvent {
    pub change: IdentityChange,
    pub actor: Principal,
    pub at: Timestamp,
}

impl Record for IdentityEvent {
    const TAG: &'static str = "identity-event";
}
