//! Intents: the "why" behind a change.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::ids::{ChangeId, IntentId};
use crate::principal::Principal;
use crate::time::Timestamp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IntentKind {
    Feature,
    Bug,
    Refactor,
    Chore,
    Experiment,
    /// Automatically created, e.g. for a quarantined flaky test.
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IntentStatus {
    Open,
    InProgress,
    Done,
    Abandoned,
}

/// Risk classification of a change, computed by policy from the change's
/// facts and overridable by an owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskClass {
    Trivial,
    Low,
    Normal,
    Sensitive,
    Critical,
}

impl RiskClass {
    pub fn as_str(self) -> &'static str {
        match self {
            RiskClass::Trivial => "trivial",
            RiskClass::Low => "low",
            RiskClass::Normal => "normal",
            RiskClass::Sensitive => "sensitive",
            RiskClass::Critical => "critical",
        }
    }

    /// One step more cautious; `Critical` is the ceiling.
    pub fn bump(self) -> RiskClass {
        match self {
            RiskClass::Trivial => RiskClass::Low,
            RiskClass::Low => RiskClass::Normal,
            RiskClass::Normal => RiskClass::Sensitive,
            RiskClass::Sensitive | RiskClass::Critical => RiskClass::Critical,
        }
    }
}

impl fmt::Display for RiskClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for RiskClass {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        match s {
            "trivial" => Ok(RiskClass::Trivial),
            "low" => Ok(RiskClass::Low),
            "normal" => Ok(RiskClass::Normal),
            "sensitive" => Ok(RiskClass::Sensitive),
            "critical" => Ok(RiskClass::Critical),
            other => Err(Error::InvalidId(format!("unknown risk class {other:?}"))),
        }
    }
}

/// A task, spec, bug or plan that changes link to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Intent {
    pub id: IntentId,
    pub title: String,
    pub body: String,
    pub kind: IntentKind,
    pub status: IntentStatus,
    pub owner: Principal,
    #[serde(default)]
    pub parent: Option<IntentId>,
    /// Risk override set by an owner; otherwise policy computes it per change.
    #[serde(default)]
    pub risk_override: Option<RiskClass>,
    #[serde(default)]
    pub changes: Vec<ChangeId>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Intent {
    pub fn new(title: impl Into<String>, kind: IntentKind, owner: Principal) -> Self {
        let now = Timestamp::now();
        Intent {
            id: IntentId::new(),
            title: title.into(),
            body: String::new(),
            kind,
            status: IntentStatus::Open,
            owner,
            parent: None,
            risk_override: None,
            changes: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn is_closed(&self) -> bool {
        matches!(self.status, IntentStatus::Done | IntentStatus::Abandoned)
    }
}

impl crate::Record for Intent {
    const TAG: &'static str = "intent";
}
