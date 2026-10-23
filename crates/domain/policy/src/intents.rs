//! Intent scopes: whether an intent is approved, and whether a change
//! stays inside the scope it was approved for.
//!
//! An intent is approved when every directory its scope covers has, at
//! the scope's current revision, as many approvals from that directory's
//! owners as its owner sets require (one when no `OWNERS.toml` covers it).
//! An agent's approval of its own intent never counts.
//! A change inherits the approval while it touches only scope paths, none
//! of the forbidden ones, stays under the per-change and per-intent
//! limits, and the day's budgets are not spent; otherwise it gets normal
//! review.

use std::collections::BTreeSet;

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use rill_types::{
    Intent, IntentScope, IntentStatus, PrincipalKind, RepoPath, RepoPathBuf, Timestamp,
};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::owners::{OwnersIndex, OWNERS_FILE};

/// What an intent contributes to a change's facts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntentFacts {
    /// Every scope directory has its owners' approvals at this revision.
    pub approved: bool,
    /// Approved, and the change stays inside the scope and its budgets.
    pub in_scope: bool,
    /// Why the change is not in scope, one line each.
    pub reasons: Vec<String>,
    /// Independent agent approvals each change needs, when set.
    pub agent_quorum: Option<u32>,
}

/// How much of an intent's day budget is used.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayCounts {
    /// Changes linked today.
    pub changes: u32,
    /// Changes landed today.
    pub lands: u32,
}

/// The directories a scope covers: each glob's literal prefix, the
/// components before the first one with a glob character.
pub fn scope_dirs(scope: &IntentScope) -> Vec<RepoPathBuf> {
    let mut out = BTreeSet::new();
    for glob in &scope.paths {
        let literal: Vec<&str> = glob
            .trim_start_matches('/')
            .split('/')
            .take_while(|c| !c.contains(['*', '?', '[', '{']))
            .collect();
        // A glob naming one file: its directory.
        let literal = if literal.len() == glob.split('/').count() {
            &literal[..literal.len().saturating_sub(1)]
        } else {
            &literal[..]
        };
        if let Ok(dir) = RepoPathBuf::new(literal.join("/")) {
            out.insert(dir);
        }
    }
    out.into_iter().collect()
}

/// Approvals each scope directory needs: the most any owner set covering
/// it requires, or one.
fn required_for(dir: &RepoPath, owners: &OwnersIndex) -> u32 {
    let probe = dir
        .to_owned_path()
        .join(OWNERS_FILE)
        .unwrap_or_else(|_| dir.to_owned_path());
    owners
        .sets_for(&probe)
        .iter()
        .map(|s| s.required)
        .max()
        .unwrap_or(1)
}

/// The scope directories still short of approvals at the intent's current
/// revision; empty when it is approved. An intent without a scope is never
/// approved.
pub fn missing_approvals(intent: &Intent, owners: &OwnersIndex) -> Option<Vec<String>> {
    let scope = intent.scope.as_ref()?;
    let mut missing = Vec::new();
    for dir in scope_dirs(scope) {
        let approvers: BTreeSet<_> = intent
            .approvals
            .iter()
            .filter(|a| a.revision == intent.revision)
            // An agent may not approve its own plan; a human owner may.
            .filter(|a| a.by.id != intent.owner.id || intent.owner.kind == PrincipalKind::Human)
            .filter(|a| a.owns.iter().any(|o| o == dir.as_str()))
            .map(|a| a.by.id)
            .collect();
        if (approvers.len() as u32) < required_for(&dir, owners) {
            missing.push(dir.into_string());
        }
    }
    Some(missing)
}

fn compile(globs: &[String]) -> Result<GlobSet> {
    let mut set = GlobSetBuilder::new();
    for g in globs {
        let glob = GlobBuilder::new(g.trim_start_matches('/'))
            .literal_separator(true)
            .build()
            .map_err(|e| Error::InvalidGlob {
                glob: g.clone(),
                reason: e.to_string(),
            })?;
        set.add(glob);
    }
    set.build().map_err(|e| Error::InvalidGlob {
        glob: globs.join(", "),
        reason: e.to_string(),
    })
}

/// Check a scope's globs, for the API to refuse a bad one up front.
pub fn validate_scope(scope: &IntentScope) -> Result<()> {
    if scope.paths.is_empty() {
        return Err(Error::PolicyParse(
            "an intent scope needs at least one path".into(),
        ));
    }
    compile(&scope.paths)?;
    compile(&scope.forbid_paths)?;
    Ok(())
}

/// Whether a change touching `paths` (of `total_files`) stays inside the
/// approved `intent`.
pub fn check_scope(
    intent: &Intent,
    approved: bool,
    paths: &[&RepoPath],
    total_files: u64,
    today: DayCounts,
    now: Timestamp,
) -> Result<IntentFacts> {
    let mut reasons = Vec::new();
    let Some(scope) = &intent.scope else {
        return Ok(IntentFacts {
            reasons: vec![format!("intent {} has no scope", intent.id)],
            ..IntentFacts::default()
        });
    };
    if !approved {
        reasons.push(format!(
            "intent {} is not approved at revision {}",
            intent.id, intent.revision
        ));
    }
    if matches!(intent.status, IntentStatus::Done | IntentStatus::Abandoned) {
        reasons.push(format!("intent {} is closed", intent.id));
    }
    if scope.expires.is_some_and(|at| now > at) {
        reasons.push(format!("intent {} expired", intent.id));
    }
    let allowed = compile(&scope.paths)?;
    let forbidden = compile(&scope.forbid_paths)?;
    for path in paths {
        if forbidden.is_match(path.as_str()) {
            reasons.push(format!("{path} is forbidden by the scope"));
        } else if !allowed.is_match(path.as_str()) {
            reasons.push(format!("{path} is outside the scope"));
        }
    }
    if let Some(max) = scope.max_files_per_change {
        if total_files > u64::from(max) {
            reasons.push(format!(
                "{total_files} files exceeds the scope's {max} per change"
            ));
        }
    }
    if let Some(max) = scope.max_changes {
        if intent.changes.len() as u64 > u64::from(max) {
            reasons.push(format!("the intent has more than its {max} changes"));
        }
    }
    if let Some(max) = scope.changes_per_day {
        if today.changes > max {
            reasons.push(format!(
                "{} changes today exceeds {max} per day",
                today.changes
            ));
        }
    }
    if let Some(max) = scope.lands_per_day {
        if today.lands >= max {
            reasons.push(format!(
                "{} landings today reaches {max} per day",
                today.lands
            ));
        }
    }
    Ok(IntentFacts {
        approved,
        in_scope: reasons.is_empty(),
        reasons,
        agent_quorum: scope.agent_quorum,
    })
}

/// The UTC day a time falls on, `YYYY-MM-DD`, for budget counters.
pub fn day_of(at: Timestamp) -> String {
    at.to_rfc3339().chars().take(10).collect()
}

/// The repository record holding an intent's [`DayCounts`] for `day`.
pub fn day_key(intent: &rill_types::IntentId, day: &str) -> String {
    format!("intent-day:{intent}:{day}")
}

#[cfg(test)]
mod tests {
    use rill_types::{IntentApproval, IntentKind, Principal};

    use super::*;
    use crate::owners::OwnersTexts;

    fn p(s: &str) -> RepoPathBuf {
        RepoPathBuf::new(s).unwrap()
    }

    fn intent(paths: &[&str]) -> Intent {
        let mut i = Intent::new("migrate", IntentKind::Refactor, Principal::agent("planner"));
        i.scope = Some(IntentScope {
            paths: paths.iter().map(|s| s.to_string()).collect(),
            forbid_paths: vec!["**/secrets/**".into()],
            max_files_per_change: Some(3),
            lands_per_day: Some(2),
            ..IntentScope::default()
        });
        i.revision = 1;
        i
    }

    fn approve(i: &mut Intent, who: &str, owns: &[&str], revision: u32) {
        i.approvals.push(IntentApproval {
            by: Principal::human(who),
            at: Timestamp::from_millis(0),
            revision,
            owns: owns.iter().map(|s| s.to_string()).collect(),
        });
    }

    #[test]
    fn scope_directories_are_the_literal_prefixes() {
        let i = intent(&["services/gateway/**", "libs/http/*.rs", "Cargo.toml"]);
        let dirs: Vec<String> = scope_dirs(i.scope.as_ref().unwrap())
            .into_iter()
            .map(|d| d.into_string())
            .collect();
        assert_eq!(dirs, ["", "libs/http", "services/gateway"]);
    }

    #[test]
    fn approval_needs_each_directorys_owners_at_the_current_revision() {
        let owners = OwnersIndex::compile(OwnersTexts(
            [(
                "a".to_string(),
                "owners = [\"team:a\"]\n[[rule]]\npaths = [\"**\"]\nowners = [\"team:a\"]\nrequired = 2".to_string(),
            )]
            .into(),
        ))
        .unwrap();
        let mut i = intent(&["a/**", "b/**"]);
        assert_eq!(missing_approvals(&i, &owners).unwrap(), ["a", "b"]);
        approve(&mut i, "ann", &["a", "b"], 1);
        assert_eq!(
            missing_approvals(&i, &owners).unwrap(),
            ["a"],
            "a's rule needs two"
        );
        approve(&mut i, "bob", &["a"], 0);
        assert_eq!(
            missing_approvals(&i, &owners).unwrap(),
            ["a"],
            "an old revision does not count"
        );
        approve(&mut i, "bob", &["a"], 1);
        assert!(missing_approvals(&i, &owners).unwrap().is_empty());
        let mut own = intent(&["b/**"]);
        own.owner = Principal::agent("ann");
        own.approvals.push(IntentApproval {
            by: Principal::agent("ann"),
            at: Timestamp::from_millis(0),
            revision: 1,
            owns: vec!["b".into()],
        });
        assert_eq!(
            missing_approvals(&own, &owners).unwrap(),
            ["b"],
            "an agent's own plan"
        );
        own.owner = Principal::human("ann");
        approve(&mut own, "ann", &["b"], 1);
        assert!(
            missing_approvals(&own, &owners).unwrap().is_empty(),
            "a human owner's own plan"
        );
    }

    #[test]
    fn a_change_leaves_the_scope_for_every_bound_it_crosses() {
        let i = intent(&["services/**"]);
        let now = Timestamp::from_millis(0);
        let ok = check_scope(
            &i,
            true,
            &[p("services/a.rs").as_path()],
            1,
            DayCounts::default(),
            now,
        )
        .unwrap();
        assert!(ok.in_scope, "{:?}", ok.reasons);
        let paths = [p("services/secrets/k.rs"), p("docs/x.md")];
        let refs: Vec<&RepoPath> = paths.iter().map(|p| p.as_path()).collect();
        let out = check_scope(
            &i,
            false,
            &refs,
            4,
            DayCounts {
                changes: 0,
                lands: 2,
            },
            now,
        )
        .unwrap();
        assert!(!out.in_scope);
        assert_eq!(out.reasons.len(), 5, "{:?}", out.reasons);
        let mut expired = i.clone();
        expired.scope.as_mut().unwrap().expires = Some(Timestamp::from_millis(-1));
        assert!(
            !check_scope(
                &expired,
                true,
                &[p("services/a.rs").as_path()],
                1,
                DayCounts::default(),
                now
            )
            .unwrap()
            .in_scope
        );
    }
}
