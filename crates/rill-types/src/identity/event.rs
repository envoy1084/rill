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
    /// A user was created or changed (status, roles, teams).
    UserChanged {
        org: String,
        name: String,
    },
    ProviderChanged {
        org: String,
        name: String,
    },
    AgentChanged {
        org: String,
        name: String,
    },
    WorkloadIssuerChanged {
        org: String,
        name: String,
    },
    /// An org's identity settings (blocked models) changed.
    OrgSettingsChanged {
        org: String,
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
