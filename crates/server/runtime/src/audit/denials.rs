//! Refusals in the audit log. A refusal changes nothing, so it is written
//! on its own, best effort, and sampled: at most one per principal and
//! repository a minute, so a client hammering a closed door cannot flood
//! the log or the store.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use rill_store::audit::AuditStore;
use rill_types::audit::{AuditOutcome, AuditRecord};
use rill_types::{PrincipalId, RepoId, Timestamp};

const EVERY: Duration = Duration::from_secs(60);

/// The sampler and where refusals go.
pub struct Denials {
    audit: AuditStore,
    last: DashMap<(PrincipalId, RepoId), Instant>,
}

impl Denials {
    pub fn new(audit: AuditStore) -> Self {
        Denials {
            audit,
            last: DashMap::new(),
        }
    }

    /// Note that `actor` was refused `action` on `target` in `repo` of
    /// `org`, unless one of its refusals there was noted within a minute.
    pub fn note(
        &self,
        org: &str,
        repo: RepoId,
        actor: &rill_types::Principal,
        action: &str,
        target: String,
        reason: String,
    ) {
        let key = (actor.id, repo);
        let now = Instant::now();
        let fresh = match self.last.get(&key) {
            Some(at) => now.duration_since(*at) >= EVERY,
            None => true,
        };
        if !fresh {
            return;
        }
        self.last.insert(key, now);
        if self.last.len() > 100_000 {
            self.last.retain(|_, at| now.duration_since(*at) < EVERY);
        }
        let record = AuditRecord {
            org: org.to_string(),
            at: Timestamp::now(),
            actor: actor.clone(),
            action: action.to_string(),
            target,
            outcome: AuditOutcome::Denied { reason },
            detail: BTreeMap::new(),
        };
        let audit = self.audit.clone();
        tokio::spawn(async move {
            if let Err(e) = audit.record(&record).await {
                tracing::warn!(error = %e, "recording a refusal failed");
            }
        });
    }
}
