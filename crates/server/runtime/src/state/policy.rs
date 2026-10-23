//! Per-repository policy: the Cedar submit rules (trunk's `.rill/policy/`
//! files, else the stored policy, else the default, with the org's
//! guardrails after them) and the risk configuration, hot-swapped into the
//! review service when they change.

use std::sync::Arc;

use bytes::Bytes;
use rill_core::{DeriveManager, RootTree};
use rill_policy::submit::with_guardrails;
use rill_policy::{CompiledRiskConfig, RiskConfig, SubmitPolicy};
use rill_store::RepoStore;
use rill_types::{EntryKind, Principal, RepoPathBuf, Timestamp};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Name of the Cedar submit policy.
pub const SUBMIT_POLICY: &str = "submit";
/// Where a repository keeps its own submit rules, on trunk.
pub const POLICY_DIR: &str = ".rill/policy";
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

/// The name `PolicyChanged` carries when an org's guardrails change.
pub const GUARDRAILS: &str = "guardrails";

/// Tell every server following `repo` that its policy `name` changed
/// elsewhere (an org's guardrails), so each reloads it.
pub async fn announce(repo: &RepoStore, name: &str, by: &Principal) -> Result<()> {
    let at = Timestamp::now();
    let event = rill_types::Event {
        kind: rill_types::EventKind::PolicyChanged {
            name: name.to_string(),
        },
        actor: by.clone(),
        at,
    };
    repo.set_misc_with_event(
        &misc_key(&format!("{name}-changed")),
        Bytes::from(at.millis().to_string()),
        &event,
    )
    .await?;
    Ok(())
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
    let event = rill_types::Event {
        kind: rill_types::EventKind::PolicyChanged {
            name: name.to_string(),
        },
        actor: policy.updated_by.clone(),
        at: policy.updated_at,
    };
    repo.set_misc_with_event(&misc_key(name), Bytes::from(bytes), &event)
        .await?;
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

/// The submit policy in force, its full text and where it came from.
pub struct EffectiveSubmit {
    pub policy: SubmitPolicy,
    pub text: String,
    pub source: String,
}

/// The `*.cedar` files in trunk's `.rill/policy/`, by path.
pub async fn trunk_policy_files(
    repo: &RepoStore,
    derive: &Arc<DeriveManager>,
) -> Result<Vec<(String, String)>> {
    let Some((_, head)) = repo.trunk_head().await? else {
        return Ok(Vec::new());
    };
    let root = derive.get::<RootTree>(&head).await?;
    let dir = RepoPathBuf::new(POLICY_DIR)?;
    let Ok(entries) = derive.trees().list_dir(root, &dir).await else {
        return Ok(Vec::new());
    };
    let mut files = Vec::new();
    for (name, kind) in entries {
        let EntryKind::File { content, .. } = kind else {
            continue;
        };
        if !name.ends_with(".cedar") {
            continue;
        }
        let bytes = rill_core::read_content(repo.objects().as_ref(), &content).await?;
        files.push((
            format!("{POLICY_DIR}/{name}"),
            String::from_utf8_lossy(&bytes).into_owned(),
        ));
    }
    Ok(files)
}

/// The repository's rules before guardrails: trunk's `.rill/policy/`
/// files, else the stored `submit` policy, else the default.
async fn repo_rules(repo: &RepoStore, derive: &Arc<DeriveManager>) -> Result<(String, String)> {
    let files = trunk_policy_files(repo, derive).await?;
    if !files.is_empty() {
        let names: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
        let source = names.join(", ");
        let text = files
            .iter()
            .map(|(p, t)| format!("// ---- {p} ----\n{t}\n"))
            .collect::<String>();
        match SubmitPolicy::from_source(&text) {
            Ok(_) => return Ok((text, source)),
            // Commits are linted file by file; a set that does not compile
            // together falls back rather than stopping review.
            Err(e) => tracing::error!(error = %e, files = %source, "ignoring trunk's policy files"),
        }
    }
    Ok(match load(repo, SUBMIT_POLICY).await? {
        Some(stored) => (stored.text, "stored".to_string()),
        None => (
            rill_policy::submit::default_policy().to_string(),
            "default".to_string(),
        ),
    })
}

/// The effective submit policy: the repository's rules with the org's
/// guardrails after them. Its text is kept by digest, so a landing's
/// recorded policy can be read back.
pub async fn effective_submit(
    repo: &RepoStore,
    derive: &Arc<DeriveManager>,
    guardrails: Option<String>,
) -> Result<EffectiveSubmit> {
    let (rules, mut source) = repo_rules(repo, derive).await?;
    let text = with_guardrails(&rules, guardrails.as_deref());
    if text != rules {
        source.push_str(" + org guardrails");
    }
    let policy = SubmitPolicy::from_source(&text)?;
    let key = policy_text_key(&policy.digest());
    if repo.get_misc(&key).await?.is_none() {
        repo.set_misc(&key, Bytes::from(text.clone())).await?;
    }
    Ok(EffectiveSubmit {
        policy,
        text,
        source,
    })
}

fn policy_text_key(digest: &[u8; 32]) -> String {
    format!("policy-text:{}", hex::encode(digest))
}

/// The policy text a digest names, when this repository ever used it.
pub async fn policy_text(repo: &RepoStore, digest: &[u8; 32]) -> Result<Option<String>> {
    Ok(repo
        .get_misc(&policy_text_key(digest))
        .await?
        .map(|b| String::from_utf8_lossy(&b).into_owned()))
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
        let derive = DeriveManager::new(
            repo.clone(),
            rill_core::TreeOps::new(repo.objects().clone(), 64),
            64,
        );
        let effective = effective_submit(&repo, &derive, None).await.unwrap();
        assert_eq!(effective.source, "default");
        assert_eq!(
            policy_text(&repo, &effective.policy.digest())
                .await
                .unwrap(),
            Some(effective.text)
        );
    }
}
