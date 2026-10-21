//! People in an organization, as sign-in and provisioning see them.

use serde::{Deserialize, Serialize};

use crate::encoding::Record;
use crate::principal::Principal;
use crate::time::Timestamp;

/// Whether an account may act.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    Active,
    /// Refused everywhere: sign-in, tokens already issued, sponsorship.
    Suspended,
}

/// A role within one organization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrgRole {
    /// Manages the org's identity providers, agents, trusts and members.
    Admin,
}

/// An identity at an external issuer that signs in as a user.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ExternalIdentity {
    pub issuer: String,
    pub subject: String,
}

/// A human member of an organization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserRecord {
    pub org: String,
    pub principal: Principal,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub external: Vec<ExternalIdentity>,
    pub status: AccountStatus,
    pub roles: Vec<OrgRole>,
    /// Team names, for `team:` relation subjects.
    pub teams: Vec<String>,
    /// The provisioning system's id for the user, when one manages it.
    pub scim_id: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Record for UserRecord {
    const TAG: &'static str = "user";
}

impl UserRecord {
    pub fn is_active(&self) -> bool {
        self.status == AccountStatus::Active
    }

    pub fn has_role(&self, role: OrgRole) -> bool {
        self.roles.contains(&role)
    }
}
