//! Whether the approvals on a change satisfy every owner set it touches.

use rill_types::{Principal, PrincipalKind, Timestamp};

use super::file::Delegation;
use super::index::OwnerSet;

/// Someone who approved, with the teams they belong to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Approver {
    pub principal: Principal,
    pub teams: Vec<String>,
}

/// One owner set's state for a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetCoverage {
    pub set: OwnerSet,
    /// Distinct approvers who count for the set.
    pub approvals: u32,
    pub covered: bool,
}

/// Whether `principal` (in `teams`) is the owner entry `owner`.
pub fn is_entry(owner: &str, principal: &Principal, teams: &[String]) -> bool {
    match owner.split_once(':') {
        Some(("user", name)) => principal.kind == PrincipalKind::Human && principal.name == name,
        Some(("agent", name)) => principal.kind == PrincipalKind::Agent && principal.name == name,
        Some(("team", name)) => teams.iter().any(|t| t == name),
        _ => false,
    }
}

/// The principal an owner entry names, when it names one (not a team).
fn entry_principal(entry: &str) -> Option<Principal> {
    match entry.split_once(':')? {
        ("user", name) => Some(Principal::human(name)),
        ("agent", name) => Some(Principal::agent(name)),
        _ => None,
    }
}

/// Whether `approver` counts for `set` at `now`: as an owner, or as the
/// delegate of an owner whose delegation is still running.
pub fn counts_for(set: &OwnerSet, approver: &Approver, now: Timestamp) -> bool {
    set.owners.iter().any(|owner| {
        is_entry(owner, &approver.principal, &approver.teams)
            || delegated(&set.delegates, owner, approver, now)
    })
}

fn delegated(delegates: &[Delegation], owner: &str, approver: &Approver, now: Timestamp) -> bool {
    delegates.iter().any(|d| {
        d.from == owner && now <= d.until && is_entry(&d.to, &approver.principal, &approver.teams)
    })
}

/// Each set's coverage by `approvers`. The author never counts for a set,
/// even as one of its owners.
pub fn coverage(
    sets: &[OwnerSet],
    approvers: &[Approver],
    author: &Principal,
    now: Timestamp,
) -> Vec<SetCoverage> {
    sets.iter()
        .map(|set| {
            let approvals = approvers
                .iter()
                .filter(|a| a.principal.id != author.id && counts_for(set, a, now))
                .count() as u32;
            SetCoverage {
                set: set.clone(),
                approvals,
                covered: approvals >= set.required,
            }
        })
        .collect()
}

/// The principals an overdue set escalates to, in order: active delegates
/// of its named owners, then its escalation contact.
pub fn escalation_targets(set: &OwnerSet, now: Timestamp) -> Vec<String> {
    let mut out: Vec<String> = set
        .delegates
        .iter()
        .filter(|d| now <= d.until && set.owners.contains(&d.from))
        .map(|d| d.to.clone())
        .collect();
    out.extend(set.escalate_to.clone());
    out.dedup();
    out
}

/// The single principals among an owner set's entries (not teams).
pub fn named_owners(set: &OwnerSet) -> Vec<Principal> {
    set.owners
        .iter()
        .filter_map(|o| entry_principal(o))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_types::RepoPathBuf;

    fn set(owners: &[&str], required: u32, delegates: Vec<Delegation>) -> OwnerSet {
        OwnerSet {
            id: "d#rule0".into(),
            dir: RepoPathBuf::root(),
            owners: owners.iter().map(|s| s.to_string()).collect(),
            required,
            sensitive: false,
            sla: None,
            escalate_to: Some("team:oncall".into()),
            delegates,
        }
    }

    fn approver(p: Principal, teams: &[&str]) -> Approver {
        Approver {
            principal: p,
            teams: teams.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn required_counts_distinct_approvers_and_never_the_author() {
        let s = set(&["team:security", "user:alice"], 2, vec![]);
        let alice = approver(Principal::human("alice"), &[]);
        let sec = approver(Principal::human("sam"), &["security"]);
        let now = Timestamp::from_millis(0);
        let one = coverage(
            std::slice::from_ref(&s),
            std::slice::from_ref(&alice),
            &Principal::human("x"),
            now,
        );
        assert!(!one[0].covered);
        let two = coverage(
            std::slice::from_ref(&s),
            &[alice.clone(), sec.clone()],
            &Principal::human("x"),
            now,
        );
        assert!(two[0].covered);
        let by_author = coverage(
            std::slice::from_ref(&s),
            &[alice, sec],
            &Principal::human("alice"),
            now,
        );
        assert_eq!(
            by_author[0].approvals, 1,
            "the author's own approval does not count"
        );
    }

    #[test]
    fn a_delegate_counts_only_while_the_delegation_runs() {
        let until = Timestamp::from_millis(1_000);
        let s = set(
            &["user:alice"],
            1,
            vec![Delegation {
                from: "user:alice".into(),
                to: "user:bob".into(),
                until,
            }],
        );
        let bob = approver(Principal::human("bob"), &[]);
        assert!(counts_for(&s, &bob, Timestamp::from_millis(999)));
        assert!(!counts_for(&s, &bob, Timestamp::from_millis(1_001)));
        assert_eq!(
            escalation_targets(&s, Timestamp::from_millis(10)),
            ["user:bob", "team:oncall"]
        );
        assert_eq!(
            escalation_targets(&s, Timestamp::from_millis(2_000)),
            ["team:oncall"]
        );
        assert!(
            !counts_for(
                &s,
                &approver(Principal::agent("alice"), &[]),
                Timestamp::from_millis(0)
            ),
            "kinds differ"
        );
    }
}
