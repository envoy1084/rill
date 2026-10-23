//! What needs a given human's attention, most urgent first: escalated
//! owner reviews, sampled audits, owner reviews waiting on a set they
//! count for, intent scopes waiting on directories they own, and threads
//! addressed to them.

use rill_policy::intents::missing_approvals;
use rill_policy::owners::{counts_for, is_entry, Approver, OWNERS_FILE};
use rill_policy::submit::OWNER_LABEL;
use rill_types::{
    ChangeId, IntentId, IntentStatus, Principal, RepoPathBuf, ReviewStatus, Timestamp, TrunkSeq,
};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::lifecycle::notes;
use crate::service::ReviewService;

/// Prefix of the review notes the escalation worker writes.
const ESCALATION_NOTE: &str = "escalation:";

/// What kind of attention an item asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InboxKind {
    Escalation,
    Audit,
    OwnerReview,
    IntentApproval,
    Thread,
}

impl InboxKind {
    fn base_urgency(self) -> u32 {
        match self {
            InboxKind::Escalation => 400,
            InboxKind::Audit => 300,
            InboxKind::OwnerReview => 200,
            InboxKind::IntentApproval => 150,
            InboxKind::Thread => 100,
        }
    }
}

/// One thing waiting on someone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxItem {
    pub kind: InboxKind,
    pub change: Option<ChangeId>,
    pub intent: Option<IntentId>,
    pub landing: Option<TrunkSeq>,
    pub title: String,
    pub detail: String,
    pub since: Timestamp,
    /// Higher first.
    pub urgency: u32,
}

/// Who is asking: their teams, and whether they hold the repository's
/// `owner` relation (which stands for owners where no `OWNERS.toml` is).
#[derive(Debug, Clone)]
pub struct Reader {
    pub approver: Approver,
    pub repo_owner: bool,
}

impl Reader {
    fn is(&self, entry: &str) -> bool {
        (entry == "owners" && self.repo_owner)
            || is_entry(entry, &self.approver.principal, &self.approver.teams)
    }

    fn me(&self) -> &Principal {
        &self.approver.principal
    }
}

fn item(kind: InboxKind, title: impl Into<String>, since: Timestamp) -> InboxItem {
    InboxItem {
        kind,
        change: None,
        intent: None,
        landing: None,
        title: title.into(),
        detail: String::new(),
        since,
        urgency: kind.base_urgency(),
    }
}

fn first_line(s: &str) -> String {
    let line = s.lines().next().unwrap_or_default();
    line.chars().take(80).collect()
}

impl ReviewService {
    /// The reader's inbox, most urgent first, at most `limit` items.
    pub async fn inbox(&self, reader: &Reader, limit: usize) -> Result<Vec<InboxItem>> {
        let now = Timestamp::now();
        let mut out = Vec::new();
        let owners = self.trunk_owners().await?;
        for state in self.repo.list_reviews().await? {
            if !matches!(state.status, ReviewStatus::Open | ReviewStatus::Landable) {
                continue;
            }
            let id = state.change_id;
            for t in state.threads.iter().filter(|t| !t.resolved) {
                if t.to.as_ref() == Some(reader.me()) {
                    let last = t.comments.last();
                    let mut i = item(
                        InboxKind::Thread,
                        first_line(last.map_or("", |c| &c.body)),
                        last.map_or(state.updated_at, |c| c.created_at),
                    );
                    i.change = Some(id);
                    out.push(i);
                }
            }
            for (set, value) in notes(&state, ESCALATION_NOTE) {
                let mut parts = value.splitn(3, '|');
                let (level, target, at) = (parts.next(), parts.next(), parts.next());
                if target.is_some_and(|t| reader.is(t)) {
                    let mut i = item(
                        InboxKind::Escalation,
                        format!("owner review of {set} is past its SLA"),
                        at.and_then(|ms| ms.parse().ok())
                            .map_or(now, Timestamp::from_millis),
                    );
                    i.change = Some(id);
                    i.detail = format!("escalation level {}", level.unwrap_or("?"));
                    i.urgency += level.and_then(|l| l.parse::<u32>().ok()).unwrap_or(0) * 10;
                    out.push(i);
                }
            }
            if owners.is_empty() || state.owner == *reader.me() {
                continue;
            }
            let voted = state
                .current_votes()
                .any(|v| v.label == OWNER_LABEL && v.value > 0 && v.voter == *reader.me());
            if voted {
                continue;
            }
            let waiting: Vec<String> = self
                .owner_coverage(&state)
                .await?
                .into_iter()
                .filter(|c| !c.covered && counts_for(&c.set, &reader.approver, now))
                .map(|c| c.set.id)
                .collect();
            if !waiting.is_empty() {
                let mut i = item(
                    InboxKind::OwnerReview,
                    format!("owner approval for {}", waiting.join(", ")),
                    state.updated_at,
                );
                i.change = Some(id);
                if state.risk >= rill_types::RiskClass::Sensitive {
                    i.urgency += 50;
                }
                out.push(i);
            }
        }
        for intent in self.repo.list_intents().await? {
            if matches!(intent.status, IntentStatus::Done | IntentStatus::Abandoned) {
                continue;
            }
            let Some(missing) = missing_approvals(&intent, &owners) else {
                continue;
            };
            let mine: Vec<String> = missing
                .into_iter()
                .filter(|dir| {
                    let probe = RepoPathBuf::new(dir).and_then(|d| d.join(OWNERS_FILE)).ok();
                    let sets = probe.map(|p| owners.sets_for(&p)).unwrap_or_default();
                    if sets.is_empty() {
                        reader.repo_owner
                    } else {
                        sets.iter().any(|s| counts_for(s, &reader.approver, now))
                    }
                })
                .collect();
            if !mine.is_empty() {
                let mut i = item(
                    InboxKind::IntentApproval,
                    format!("approve intent {:?}", intent.title),
                    intent.updated_at,
                );
                i.intent = Some(intent.id);
                i.detail = format!("scope directories: {}", mine.join(", "));
                i.urgency += (intent.changes.len() as u32).min(50);
                out.push(i);
            }
        }
        for audit in self.pending_audits().await? {
            if audit.assignees.iter().any(|a| reader.is(a)) {
                let mut i = item(
                    InboxKind::Audit,
                    format!("audit trunk@{}, landed on agent review", audit.seq.0),
                    audit.sampled_at,
                );
                i.change = Some(audit.change);
                i.landing = Some(audit.seq);
                i.detail = format!("risk {}", audit.risk);
                out.push(i);
            }
        }
        out.sort_by(|a, b| b.urgency.cmp(&a.urgency).then(a.since.cmp(&b.since)));
        out.truncate(limit);
        Ok(out)
    }
}
