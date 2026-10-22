//! A workload's JWT for the agent a binding says it is (RFC 8693).

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;
use rill_store::identity::IdentityStore;
use rill_types::identity::{AgentRecord, IssuerKeys, WorkloadIssuer};
use rill_types::Timestamp;

use super::standing::agent_may_act;
use crate::error::{Error, Result};
use crate::oidc::{unverified_issuer, Claims, Issuers, JwksCache, JwksSource, JwtVerifier};

/// The key cache of one workload issuer, by `(org, name)`, with the key
/// source it was built from.
type KeyCaches = HashMap<(String, String), (IssuerKeys, Arc<JwksCache>)>;

/// Key caches for workload issuers with keys given by URL or inline.
#[derive(Clone)]
pub struct WorkloadKeys {
    issuers: Issuers,
    caches: Arc<Mutex<KeyCaches>>,
}

impl WorkloadKeys {
    pub fn new(issuers: Issuers) -> Self {
        WorkloadKeys {
            issuers,
            caches: Arc::default(),
        }
    }

    async fn verifier(&self, issuer: &WorkloadIssuer) -> Result<JwtVerifier> {
        let source = match &issuer.keys {
            IssuerKeys::Discovery => {
                return self
                    .issuers
                    .verifier(&issuer.issuer, issuer.audiences.clone())
                    .await
            }
            IssuerKeys::Uri(uri) => JwksSource::Uri(uri.clone()),
            IssuerKeys::Inline(doc) => JwksSource::Inline(doc.clone()),
        };
        let key = (issuer.org.clone(), issuer.name.clone());
        let cache = {
            let mut caches = self.caches.lock();
            match caches.get(&key) {
                Some((keys, cache)) if *keys == issuer.keys => cache.clone(),
                _ => {
                    let cache = Arc::new(JwksCache::new(self.issuers.client().clone(), source));
                    caches.insert(key, (issuer.keys.clone(), cache.clone()));
                    cache
                }
            }
        };
        Ok(JwtVerifier::new(
            &issuer.issuer,
            issuer.audiences.clone(),
            cache,
        ))
    }
}

/// The agent a workload `token` acts as in `org`, after checking it may.
/// `agent` names it when several bindings could match.
pub async fn exchange_workload(
    store: &IdentityStore,
    keys: &WorkloadKeys,
    org: &str,
    token: &str,
    agent: Option<&str>,
    model: Option<&str>,
    now: Timestamp,
) -> Result<(AgentRecord, Claims)> {
    let issuer_url = unverified_issuer(token)?;
    let trusted = store
        .workload_issuers(org)
        .await?
        .into_iter()
        .find(|i| i.issuer == issuer_url)
        .ok_or_else(|| {
            Error::Denied(format!("org {org:?} does not trust issuer {issuer_url:?}"))
        })?;
    let claims = keys.verifier(&trusted).await?.verify(token, None).await?;
    let subject = claims.subject()?.to_string();
    let claim = |name: &str| claims.str(name).map(String::from);
    let bound = |a: &AgentRecord| {
        a.workloads
            .iter()
            .any(|b| b.matches(&trusted.issuer, &subject, claim))
    };
    let mut candidates: Vec<AgentRecord> = match agent {
        Some(name) => store.agent(org, name).await?.into_iter().collect(),
        None => store.agents(org).await?,
    };
    candidates.retain(bound);
    let agent = match candidates.len() {
        1 => candidates.pop().expect("one"),
        0 => {
            return Err(Error::Denied(format!(
                "no agent in {org:?} is bound to {subject:?} from {issuer_url:?}"
            )))
        }
        _ => {
            return Err(Error::Denied(
                "several agents are bound to this workload; name one".into(),
            ))
        }
    };
    let settings = store.org_settings(org).await?;
    agent_may_act(store, &agent, model, &settings, now).await?;
    Ok((agent, claims))
}
