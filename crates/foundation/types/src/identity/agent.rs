//! Agents registered in an organization: who sponsors them, what they may
//! touch, which workloads may act as them, and what they may spend.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::user::AccountStatus;
use crate::encoding::Record;
use crate::principal::Principal;
use crate::time::Timestamp;

/// Who answers for an agent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sponsor {
    /// A user of the agent's org, by name.
    User(String),
    /// A team of the agent's org; active while any active member has it.
    Team(String),
}

impl Sponsor {
    /// `user:<name>` or `team:<name>`.
    pub fn label(&self) -> String {
        match self {
            Sponsor::User(name) => format!("user:{name}"),
            Sponsor::Team(name) => format!("team:{name}"),
        }
    }
}

/// A workload identity that may act as the agent: a JWT from `issuer`
/// whose subject matches and whose listed claims equal the given values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkloadBinding {
    pub issuer: String,
    /// The `sub` claim; a trailing `*` matches any suffix.
    pub subject: String,
    /// Further claims that must hold exactly (`repository`, `namespace`).
    pub claims: BTreeMap<String, String>,
}

impl WorkloadBinding {
    /// Whether `subject` (with `claims`) from `issuer` is this binding.
    pub fn matches(
        &self,
        issuer: &str,
        subject: &str,
        claim: impl Fn(&str) -> Option<String>,
    ) -> bool {
        let subject_ok = match self.subject.strip_suffix('*') {
            Some(prefix) => subject.starts_with(prefix),
            None => subject == self.subject,
        };
        issuer == self.issuer
            && subject_ok
            && self
                .claims
                .iter()
                .all(|(k, v)| claim(k).as_deref() == Some(v.as_str()))
    }
}

/// Per-day and per-minute limits; `None` is unlimited.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentBudgets {
    pub versions_per_day: Option<u64>,
    pub lands_per_day: Option<u64>,
    pub requests_per_minute: Option<u64>,
    /// Actions it may run on remote execution per day (cache hits are free).
    pub executions_per_day: Option<u64>,
}

/// A registered agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRecord {
    pub org: String,
    pub principal: Principal,
    pub sponsor: Sponsor,
    pub purpose: String,
    /// Repository names in the org it may use; `*` for all.
    pub repos: Vec<String>,
    /// Path prefixes it may use in them; empty for everything.
    pub paths: Vec<String>,
    /// Models it may run as; empty for any.
    pub models: Vec<String>,
    pub budgets: AgentBudgets,
    pub workloads: Vec<WorkloadBinding>,
    pub status: AccountStatus,
    /// The registration ends then.
    pub expires_at: Option<Timestamp>,
    pub created_by: Principal,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    /// Present when the agent reviews changes for repositories that route
    /// work to it.
    #[serde(default)]
    pub reviewer: Option<ReviewerProfile>,
}

/// How an agent reviews: what it runs and how far its vote reaches.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewerProfile {
    /// The model's family (`claude`, …): reviewers in one quorum must
    /// differ in family or prompt.
    pub model_family: String,
    /// The model it runs, recorded on every result.
    pub model: String,
    /// Its prompt's path in the repository (default
    /// `.rill/reviewers/<name>/prompt.md`).
    pub prompt: String,
    /// The largest `Review` vote it may cast, positive or negative.
    pub max_vote: i8,
    /// Reviews it may run per day, across repositories.
    pub runs_per_day: Option<u64>,
}

impl Record for AgentRecord {
    const TAG: &'static str = "agent";
}

impl AgentRecord {
    /// Whether the registration lets the agent act at `now`.
    pub fn is_live_at(&self, now: Timestamp) -> bool {
        self.status == AccountStatus::Active && self.expires_at.is_none_or(|t| now < t)
    }

    /// Whether the agent may run as `model` (any, without an allow-list).
    pub fn allows_model(&self, model: Option<&str>) -> bool {
        self.models.is_empty() || model.is_some_and(|m| self.models.iter().any(|a| a == m))
    }
}
