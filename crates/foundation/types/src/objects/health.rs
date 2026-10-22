//! Trunk health: incidents opened when post-land verification finds a
//! culprit, and the state trunk is in because of them.

use serde::{Deserialize, Serialize};

use crate::ids::ChangeId;
use crate::objects::change::ChangeVersionHash;
use crate::objects::refs::TrunkSeq;
use crate::principal::Principal;
use crate::time::Timestamp;

/// Where an incident is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IncidentState {
    /// Trunk is red because of it and nothing has landed to fix it.
    Open,
    /// A revert or forward fix landed; waiting for verification to pass.
    Fixing,
    /// Trunk passed again.
    Closed,
}

impl IncidentState {
    pub fn as_str(self) -> &'static str {
        match self {
            IncidentState::Open => "open",
            IncidentState::Fixing => "fixing",
            IncidentState::Closed => "closed",
        }
    }
}

/// How an incident ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Resolution {
    /// The culprit was reverted at this position.
    Reverted(TrunkSeq),
    /// A change naming the culprit as fixed landed at this position.
    Fixed(TrunkSeq),
    /// Trunk passed again without either, at this position.
    Recovered(TrunkSeq),
}

/// A trunk break and its culprit: one per culprit position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Incident {
    pub culprit: TrunkSeq,
    pub version: ChangeVersionHash,
    pub change: ChangeId,
    /// Failing targets (`step:…`, `test:…`) and the steps they belong to.
    pub failures: Vec<String>,
    pub steps: Vec<String>,
    /// What verification said, with the end of the failing output.
    pub details: String,
    /// Where the culprit came from, for notifications and mass reverts.
    pub author: Principal,
    pub session: Option<String>,
    pub model: Option<String>,
    pub sponsor: Option<Principal>,
    pub state: IncidentState,
    pub opened_at: Timestamp,
    pub updated_at: Timestamp,
    /// The landing that reverted or fixed it, once one landed.
    pub fix: Option<TrunkSeq>,
    pub resolution: Option<Resolution>,
    /// Why its automatic revert could not land, if it could not.
    pub revert_blocked: Option<String>,
    /// Who was asked to fix it (a fix agent), if anyone.
    pub assigned: Option<String>,
}

impl Incident {
    /// Its name: `trunk@N` of its culprit.
    pub fn id(&self) -> String {
        self.culprit.to_string()
    }

    pub fn is_open(&self) -> bool {
        self.state != IncidentState::Closed
    }
}

impl crate::Record for Incident {
    const TAG: &'static str = "incident";
}

/// What trunk is like.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum HealthState {
    #[default]
    Green,
    /// Red with a culprit known.
    Red,
    /// Every open incident has a revert or fix landed.
    Fixing,
    /// Red too long or too often: landings that touch what fails wait.
    Guarded,
}

impl HealthState {
    pub fn as_str(self) -> &'static str {
        match self {
            HealthState::Green => "green",
            HealthState::Red => "red",
            HealthState::Fixing => "fixing",
            HealthState::Guarded => "guarded",
        }
    }
}

/// Trunk's health, as the post-land verifier last judged it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TrunkHealth {
    pub state: HealthState,
    /// When it entered `state`; when it went red, for a red-like state.
    pub since: Timestamp,
    /// Culprits of the open incidents.
    pub open: Vec<TrunkSeq>,
    /// Steps failing because of them.
    pub failing_steps: Vec<String>,
    /// Frozen by an administrator: only emergency landings land.
    pub frozen: bool,
    pub frozen_reason: Option<String>,
}

impl crate::Record for TrunkHealth {
    const TAG: &'static str = "trunk-health";
}
