use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ids::PrincipalId;

/// What kind of actor a principal is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PrincipalKind {
    Human,
    Agent,
    Service,
}

impl PrincipalKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PrincipalKind::Human => "human",
            PrincipalKind::Agent => "agent",
            PrincipalKind::Service => "service",
        }
    }
}

/// An authenticated actor: a human, an agent workload, or a service.
///
/// Principals are recorded on everything they do (authored changes, votes,
/// landings) so attribution is structural rather than a commit-message
/// convention.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Principal {
    pub id: PrincipalId,
    pub kind: PrincipalKind,
    /// Human-readable handle: `alice`, `agent:reviewer-7`, `svc:lander`.
    pub name: String,
    /// Email for humans, SPIFFE id for workloads; informational.
    #[serde(default)]
    pub identity: Option<String>,
}

impl Principal {
    pub fn human(name: impl Into<String>) -> Self {
        let name = name.into();
        Principal {
            id: PrincipalId::derived("human", &name),
            kind: PrincipalKind::Human,
            name,
            identity: None,
        }
    }

    pub fn agent(name: impl Into<String>) -> Self {
        let name = name.into();
        Principal {
            id: PrincipalId::derived("agent", &name),
            kind: PrincipalKind::Agent,
            name,
            identity: None,
        }
    }

    pub fn service(name: impl Into<String>) -> Self {
        let name = name.into();
        Principal {
            id: PrincipalId::derived("service", &name),
            kind: PrincipalKind::Service,
            name,
            identity: None,
        }
    }

    pub fn is_agent(&self) -> bool {
        self.kind == PrincipalKind::Agent
    }
}

impl fmt::Display for Principal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            PrincipalKind::Human => f.write_str(&self.name),
            PrincipalKind::Agent => write!(f, "agent:{}", self.name),
            PrincipalKind::Service => write!(f, "svc:{}", self.name),
        }
    }
}
