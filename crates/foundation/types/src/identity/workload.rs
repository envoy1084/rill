//! Workload identity issuers an organization trusts: Kubernetes service
//! account issuers, CI (GitHub Actions), cloud identity, SPIFFE JWT-SVIDs.

use serde::{Deserialize, Serialize};

use crate::encoding::Record;
use crate::time::Timestamp;

/// Where a workload issuer's signing keys come from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssuerKeys {
    /// OIDC discovery at the issuer.
    Discovery,
    /// A JWKS URL.
    Uri(String),
    /// A JWKS document, for issuers whose keys are not reachable from the
    /// server (a cluster's service account issuer).
    Inline(String),
}

/// An issuer whose JWTs may be exchanged for agent tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkloadIssuer {
    pub org: String,
    pub name: String,
    pub issuer: String,
    pub keys: IssuerKeys,
    /// Accepted `aud` values; tokens must be minted for one of them.
    pub audiences: Vec<String>,
    pub created_at: Timestamp,
}

impl Record for WorkloadIssuer {
    const TAG: &'static str = "workload-issuer";
}

/// Instance-wide controls an organization applies to its agents.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrgIdentitySettings {
    pub org: String,
    /// Models whose agents are refused (the model kill switch).
    pub blocked_models: Vec<String>,
    /// Cedar `forbid` rules applied above every repository's submit policy.
    #[serde(default)]
    pub guardrails: Option<String>,
}

impl Record for OrgIdentitySettings {
    const TAG: &'static str = "org-identity";
}
