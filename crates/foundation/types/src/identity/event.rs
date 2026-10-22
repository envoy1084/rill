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
    TeamChanged {
        org: String,
        name: String,
    },
    /// An org's identity settings (blocked models) changed.
    OrgSettingsChanged {
        org: String,
    },
    /// A sign-in or workload exchange opened a session.
    SessionOpened {
        org: String,
        principal: Principal,
        session: SessionId,
        kind: String,
    },
    /// A token was issued by request (`IssueToken`) or minted by an
    /// operator; access tokens a session refreshes are covered by its
    /// `SessionOpened`.
    TokenIssued {
        org: String,
        principal: Principal,
        token_id: TokenId,
        expires_at: Timestamp,
        /// The capability's repository and paths, when narrowed.
        scope: Option<String>,
    },
    OrgCreated {
        org: String,
    },
    ScimCredentialChanged {
        org: String,
        id: String,
        active: bool,
    },
    RedactionRequested {
        org: String,
        id: String,
        hash: crate::hash::ObjectHash,
    },
    /// Every server stops serving the object from now on.
    RedactionApplied {
        org: String,
        id: String,
        hash: crate::hash::ObjectHash,
    },
    /// An org's data keys changed (added or rewrapped).
    DataKeysChanged {
        org: String,
    },
    /// A quarantined version was released by an owner.
    QuarantineCleared {
        org: String,
        repo: String,
        version: crate::hash::ObjectHash,
    },
    /// An org secret was set or deleted; the log holds only its name.
    SecretChanged {
        org: String,
        name: String,
        deleted: bool,
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
