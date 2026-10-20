//! Per-repository policy records: the Cedar submit policy and the risk
//! configuration, stored as JSON in the repository's misc keyspace and
//! hot-swapped into the review service when changed.

use bytes::Bytes;
use rill_policy::{CompiledRiskConfig, RiskConfig, SubmitPolicy};
use rill_store::RepoStore;
use rill_types::{Principal, Timestamp};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Name of the Cedar submit policy.
pub const SUBMIT_POLICY: &str = "submit";
/// Name of the risk configuration (JSON `RiskConfig`).
pub const RISK_POLICY: &str = "risk";

/// A stored policy document with its provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredPolicy {
    /// Policy text (Cedar for `submit`, JSON for `risk`).
    pub text: String,
    /// When it was last set.
    pub updated_at: Timestamp,
    /// Who set it.
    pub updated_by: Principal,
}

fn misc_key(name: &str) -> String {
    format!("policy:{name}")
}

/// Read a stored policy, if one was ever set.
pub async fn load(repo: &RepoStore, name: &str) -> Result<Option<StoredPolicy>> {
    match repo.get_misc(&misc_key(name)).await? {
        Some(bytes) => Ok(Some(
            serde_json::from_slice(&bytes).map_err(|e| Error::internal(e.to_string()))?,
        )),
        None => Ok(None),
    }
}

/// Persist a policy document.
pub async fn store(repo: &RepoStore, name: &str, policy: &StoredPolicy) -> Result<()> {
    let bytes = serde_json::to_vec(policy).map_err(|e| Error::internal(e.to_string()))?;
    repo.set_misc(&misc_key(name), Bytes::from(bytes)).await?;
    Ok(())
}

/// Parse and validate the text of the policy called `name`. Validation is
/// the whole point: a bad policy is rejected before it is stored.
pub fn validate(name: &str, text: &str) -> Result<()> {
    match name {
        SUBMIT_POLICY => SubmitPolicy::from_source(text)
            .map(|_| ())
            .map_err(Error::from),
        RISK_POLICY => parse_risk(text).and_then(|r| r.compile().map_err(Error::from).map(|_| ())),
        other => Err(Error::invalid(format!(
            "unknown policy {other:?}; expected {SUBMIT_POLICY:?} or {RISK_POLICY:?}"
        ))),
    }
}

/// Parse a JSON risk configuration.
pub fn parse_risk(text: &str) -> Result<RiskConfig> {
    serde_json::from_str(text).map_err(|e| Error::invalid(format!("risk config: {e}")))
}

/// The effective submit policy: the stored one, or the crate default.
pub async fn effective_submit(repo: &RepoStore) -> Result<SubmitPolicy> {
    match load(repo, SUBMIT_POLICY).await? {
        Some(stored) => Ok(SubmitPolicy::from_source(&stored.text)?),
        None => Ok(SubmitPolicy::default_rules()),
    }
}

/// The effective risk configuration: the stored one, or `fallback` from
/// the server configuration.
pub async fn effective_risk(repo: &RepoStore, fallback: &RiskConfig) -> Result<CompiledRiskConfig> {
    let config = match load(repo, RISK_POLICY).await? {
        Some(stored) => parse_risk(&stored.text)?,
        None => fallback.clone(),
    };
    Ok(config.compile()?)
}

/// The text a `GetPolicy` call returns when nothing was stored.
pub fn default_text(name: &str, risk: &RiskConfig) -> Result<String> {
    match name {
        SUBMIT_POLICY => Ok(rill_policy::submit::default_policy().to_string()),
        RISK_POLICY => {
            serde_json::to_string_pretty(risk).map_err(|e| Error::internal(e.to_string()))
        }
        other => Err(Error::not_found(format!("policy {other:?}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_store::Stores;

    #[tokio::test]
    async fn stores_and_validates() {
        let stores = Stores::memory();
        let config = stores.create_repo("r", "").await.unwrap();
        let repo = stores.repo(config.id);
        assert!(load(&repo, SUBMIT_POLICY).await.unwrap().is_none());
        assert!(validate(SUBMIT_POLICY, "permit(principal, action, resource").is_err());
        assert!(validate(RISK_POLICY, "{").is_err());
        assert!(validate("nope", "").is_err());
        let text = default_text(RISK_POLICY, &RiskConfig::default()).unwrap();
        validate(RISK_POLICY, &text).unwrap();
        let stored = StoredPolicy {
            text,
            updated_at: Timestamp::from_millis(1),
            updated_by: Principal::human("a"),
        };
        store(&repo, RISK_POLICY, &stored).await.unwrap();
        assert_eq!(load(&repo, RISK_POLICY).await.unwrap(), Some(stored));
        effective_risk(&repo, &RiskConfig::default()).await.unwrap();
        effective_submit(&repo).await.unwrap();
    }
}
