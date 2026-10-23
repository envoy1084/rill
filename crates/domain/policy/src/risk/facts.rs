//! The facts about a change that risk classification and submit rules read.

use std::collections::BTreeMap;

use rill_types::{IntentKind, Principal, Provenance, RepoPathBuf, RiskClass, Timestamp};
use serde::{Deserialize, Serialize};

use crate::directory::Person;
use crate::intents::IntentFacts;
use crate::owners::OwnerSet;

/// What happened to one path in the change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathFacts {
    /// The path touched.
    pub path: RepoPathBuf,
    /// The path did not exist in the parent.
    pub added: bool,
    /// The path existed and its content changed.
    pub modified: bool,
    /// The path no longer exists.
    pub deleted: bool,
    /// The path is left in conflict.
    pub conflict: bool,
    /// Lines added, when a text diff is available.
    pub lines_added: u64,
    /// Lines removed, when a text diff is available.
    pub lines_removed: u64,
    /// `Some(true)` when the diff is known to touch only comments, docs or
    /// formatting; `Some(false)` when it is known to touch code; `None` when
    /// nobody computed it.
    pub cosmetic_only: Option<bool>,
}

impl PathFacts {
    /// A modified path with unknown diff details.
    pub fn modified(path: RepoPathBuf) -> Self {
        PathFacts {
            path,
            added: false,
            modified: true,
            deleted: false,
            conflict: false,
            lines_added: 0,
            lines_removed: 0,
            cosmetic_only: None,
        }
    }

    /// An added path.
    pub fn added(path: RepoPathBuf) -> Self {
        PathFacts {
            added: true,
            modified: false,
            ..Self::modified(path)
        }
    }

    /// A deleted path.
    pub fn deleted(path: RepoPathBuf) -> Self {
        PathFacts {
            deleted: true,
            modified: false,
            ..Self::modified(path)
        }
    }

    /// A path left in conflict.
    pub fn conflicted(path: RepoPathBuf) -> Self {
        PathFacts {
            conflict: true,
            ..Self::modified(path)
        }
    }

    /// Set the line deltas.
    pub fn with_lines(mut self, added: u64, removed: u64) -> Self {
        self.lines_added = added;
        self.lines_removed = removed;
        self
    }

    /// Record whether the diff is cosmetic only.
    pub fn with_cosmetic_only(mut self, cosmetic: bool) -> Self {
        self.cosmetic_only = Some(cosmetic);
        self
    }

    /// Total lines changed on this path.
    pub fn lines_changed(&self) -> u64 {
        self.lines_added + self.lines_removed
    }
}

/// Everything the policy layer knows about one change version.
///
/// This is a projection assembled by the caller (review or lander) from the
/// change version, its diff against the parent, and its intent. Policy never
/// reads the object store itself, which keeps evaluation deterministic and
/// cheap enough to run on every vote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeFacts {
    /// Who authored the version.
    pub author: Principal,
    /// Signed provenance of the version.
    pub provenance: Provenance,
    /// Per-path facts, one entry per path touched.
    pub paths: Vec<PathFacts>,
    /// Total files touched. Normally `paths.len()`, but a caller that
    /// truncates `paths` for a huge change still reports the true count.
    pub total_files: u64,
    /// Total lines changed across all paths, when known.
    pub total_lines: u64,
    /// Kind of the linked intent, if any.
    pub intent_kind: Option<IntentKind>,
    /// Owner-set risk override on the linked intent, if any.
    pub risk_override: Option<RiskClass>,
    /// Depth of this change in its stack (0 for a standalone change).
    pub stack_depth: u32,
    /// The owner sets covering the touched paths, by trunk's `OWNERS.toml`
    /// files.
    pub owner_sets: Vec<OwnerSet>,
    /// What the directory says about the people involved (author, voters),
    /// keyed by their display form.
    pub people: BTreeMap<String, Person>,
    /// The linked intent's approval and whether the change is in its scope.
    pub intent: Option<IntentFacts>,
    /// When the facts were taken: time rules (weekends, hours) read it, so
    /// a recorded decision replays the same.
    pub at: Timestamp,
    /// Trunk's health then (`green`, `red`, `fixing`, `guarded`).
    pub trunk_state: String,
    /// Whether trunk was frozen.
    pub frozen: bool,
}

impl ChangeFacts {
    /// Facts for a change with the given author and paths; totals are
    /// derived from `paths`.
    pub fn new(author: Principal, paths: Vec<PathFacts>) -> Self {
        let total_files = paths.len() as u64;
        let total_lines = paths.iter().map(PathFacts::lines_changed).sum();
        ChangeFacts {
            author,
            provenance: Provenance::default(),
            paths,
            total_files,
            total_lines,
            intent_kind: None,
            risk_override: None,
            stack_depth: 0,
            owner_sets: Vec::new(),
            people: BTreeMap::new(),
            intent: None,
            at: Timestamp::from_millis(0),
            trunk_state: "green".into(),
            frozen: false,
        }
    }

    /// Whether `at` falls on a Saturday or Sunday, UTC.
    pub fn weekend(&self) -> bool {
        // Day 0 of the epoch was a Thursday (index 3 from Monday).
        let day = self.at.millis().div_euclid(86_400_000);
        (day + 3).rem_euclid(7) >= 5
    }

    /// The UTC hour of `at`.
    pub fn hour(&self) -> u32 {
        (self.at.millis().rem_euclid(86_400_000) / 3_600_000) as u32
    }

    /// Attach the owner sets of the touched paths.
    pub fn with_owner_sets(mut self, sets: Vec<OwnerSet>) -> Self {
        self.owner_sets = sets;
        self
    }

    /// What is known about `principal`; nothing when the directory was not
    /// asked.
    pub fn person(&self, principal: &Principal) -> Person {
        self.people
            .get(&principal.to_string())
            .cloned()
            .unwrap_or_default()
    }

    /// Attach provenance.
    pub fn with_provenance(mut self, provenance: Provenance) -> Self {
        self.provenance = provenance;
        self
    }

    /// Attach intent facts.
    pub fn with_intent(mut self, kind: IntentKind, risk_override: Option<RiskClass>) -> Self {
        self.intent_kind = Some(kind);
        self.risk_override = risk_override;
        self
    }

    /// Whether the conflict pipeline's agent stage produced this version.
    pub fn resolved_by_agent(&self) -> bool {
        self.provenance.resolved_by.as_deref() == Some("agent")
    }

    /// Whether any path is left in conflict.
    pub fn has_conflicts(&self) -> bool {
        self.paths.iter().any(|p| p.conflict)
    }
}
