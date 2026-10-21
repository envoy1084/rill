//! Teams of an organization, as a provisioning system manages them.

use serde::{Deserialize, Serialize};

use crate::encoding::Record;
use crate::time::Timestamp;

/// A team: a SCIM group. Members' `teams` follow its membership.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamRecord {
    pub org: String,
    pub name: String,
    /// The provisioning system's id for the group.
    pub scim_id: String,
    pub external_id: Option<String>,
    /// Member user names.
    pub members: Vec<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Record for TeamRecord {
    const TAG: &'static str = "team";
}
