//! Identity, token, relation, policy and health records.

use rill_proto::convert::{required, timestamp_from_proto};
use rill_proto::rill::v1 as pb;
use rill_types::{Principal, Timestamp};

use crate::error::{Error, Result};

/// A capability a token grants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capability {
    /// Action name, e.g. `repo.write`.
    pub action: String,
    /// Object pattern, e.g. `repo:main/*`.
    pub scope: String,
    /// Expiry, if the capability is time-bound.
    pub expires_at: Option<Timestamp>,
}

impl Capability {
    /// A capability without expiry.
    pub fn new(action: impl Into<String>, scope: impl Into<String>) -> Self {
        Capability {
            action: action.into(),
            scope: scope.into(),
            expires_at: None,
        }
    }

    /// Wire encoding.
    pub fn to_proto(&self) -> pb::Capability {
        pb::Capability {
            action: self.action.clone(),
            scope: self.scope.clone(),
            expires_at: self.expires_at.map(|t| t.millis()),
        }
    }
}

impl From<pb::Capability> for Capability {
    fn from(c: pb::Capability) -> Self {
        Capability {
            action: c.action,
            scope: c.scope,
            expires_at: c.expires_at.map(timestamp_from_proto),
        }
    }
}

/// The caller's identity as the server sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhoAmI {
    /// The authenticated principal.
    pub principal: Principal,
    /// Teams the principal belongs to.
    pub teams: Vec<String>,
    /// Capabilities the token grants.
    pub capabilities: Vec<Capability>,
    /// Token expiry.
    pub token_expires_at: Option<Timestamp>,
    /// The token's id (`tk_…`), when the caller holds an issued token.
    pub token_id: Option<String>,
    /// The session (`ss_…`) the token belongs to.
    pub session: Option<String>,
    /// The caller's organization.
    pub org: Option<String>,
    /// Who the caller acts for, when it is an agent.
    pub sponsor: Option<String>,
    /// The model the caller runs as, when it is an agent.
    pub model: Option<String>,
}

impl TryFrom<pb::WhoAmIResponse> for WhoAmI {
    type Error = Error;

    fn try_from(r: pb::WhoAmIResponse) -> Result<Self> {
        Ok(WhoAmI {
            principal: required("whoami.principal", r.principal)?.try_into()?,
            teams: r.teams,
            capabilities: r.capabilities.into_iter().map(Capability::from).collect(),
            token_expires_at: r.token_expires_at.map(timestamp_from_proto),
            token_id: r.token_id,
            session: r.session,
            org: r.org,
            sponsor: r.sponsor,
            model: r.model,
        })
    }
}

/// Inputs to mint an attenuated token.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TokenRequest {
    /// Capabilities to attenuate to; must be a subset of the caller's.
    pub capabilities: Vec<Capability>,
    /// Expiry.
    pub expires_at: Option<Timestamp>,
    /// Principal the token acts as; defaults to the caller.
    pub for_principal: Option<Principal>,
}

/// A minted token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedToken {
    /// The token string.
    pub token: String,
    /// When it expires.
    pub expires_at: Timestamp,
}

/// A named Cedar policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    /// Policy name.
    pub name: String,
    /// Cedar policy text.
    pub text: String,
    /// Last update time.
    pub updated_at: Timestamp,
    /// Who last updated it.
    pub updated_by: Principal,
}

impl TryFrom<pb::GetPolicyResponse> for Policy {
    type Error = Error;

    fn try_from(r: pb::GetPolicyResponse) -> Result<Self> {
        Ok(Policy {
            name: r.name,
            text: r.text,
            updated_at: timestamp_from_proto(r.updated_at),
            updated_by: required("policy.updated_by", r.updated_by)?.try_into()?,
        })
    }
}

/// Health of one server component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentHealth {
    /// Component name.
    pub name: String,
    /// True when healthy.
    pub ok: bool,
    /// Detail when unhealthy.
    pub detail: Option<String>,
}

/// Server health.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Health {
    /// True when every component is healthy.
    pub ok: bool,
    /// Server version.
    pub version: String,
    /// Per-component health.
    pub components: Vec<ComponentHealth>,
}

impl From<pb::HealthResponse> for Health {
    fn from(r: pb::HealthResponse) -> Self {
        Health {
            ok: r.ok,
            version: r.version,
            components: r
                .components
                .into_iter()
                .map(|c| ComponentHealth {
                    name: c.name,
                    ok: c.ok,
                    detail: c.detail,
                })
                .collect(),
        }
    }
}
