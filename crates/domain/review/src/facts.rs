//! Building policy facts from a change version and its review state.

use std::collections::BTreeSet;

use bytes::Bytes;
use rill_core::{diff_lines, diff_trees, is_binary, read_content, RootTree, TreeDiffEntry};
use rill_policy::{classify, ChangeFacts, PathFacts, SubmitFacts};
use rill_types::{ChangeVersion, ChangeVersionHash, ContentRef, Principal, ReviewState};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::service::ReviewService;

/// Files larger than this on either side are not line-diffed; their facts
/// carry no line counts and an unknown `cosmetic_only`.
pub const MAX_DIFF_BYTES: u64 = 1024 * 1024;

/// Derived-data kind under which per-version path facts are cached.
const PATH_FACTS_KIND: &str = "review-paths@1";

/// Cached projection of a version's diff; everything else in
/// [`ChangeFacts`] is cheap to rebuild or depends on mutable records.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedPaths {
    paths: Vec<PathFacts>,
}

impl ReviewService {
    /// Policy facts for the review's current version, including its depth
    /// in the stack, the owner sets it touches and what the directory says
    /// about the people involved.
    pub async fn change_facts(&self, state: &ReviewState) -> Result<ChangeFacts> {
        self.change_facts_with(state, &[]).await
    }

    /// [`ReviewService::change_facts`], also knowing `acting` (who is about
    /// to vote), whose approval the facts must be able to count.
    pub async fn change_facts_with(
        &self,
        state: &ReviewState,
        acting: &[Principal],
    ) -> Result<ChangeFacts> {
        let version = state
            .current_version()
            .ok_or(Error::NoVersions(state.change_id))?;
        let mut facts = self.version_facts(&version).await?;
        facts.stack_depth = state
            .stack
            .iter()
            .position(|c| *c == state.change_id)
            .map_or(0, |i| i as u32);
        let owners = self.trunk_owners().await?;
        if !owners.is_empty() {
            facts.owner_sets = owners.sets_for_all(facts.paths.iter().map(|p| p.path.as_path()));
        }
        let linked = self.derive.change(&version).await?.intent.or(state.intent);
        if let Some(intent) = linked {
            facts.intent = self.intent_facts(&intent, &facts, &owners).await?;
        }
        let people = std::iter::once(&state.owner)
            .chain(std::iter::once(&facts.author))
            .chain(state.current_votes().map(|v| &v.voter))
            .chain(acting.iter());
        for person in people {
            if let std::collections::btree_map::Entry::Vacant(slot) =
                facts.people.entry(person.to_string())
            {
                slot.insert(self.directory.person(person).await?);
            }
        }
        self.fill_trust(&mut facts).await?;
        let health = self.repo.trunk_health().await?;
        facts.at = rill_types::Timestamp::now();
        facts.trunk_state = health.state.as_str().to_string();
        facts.frozen = health.frozen;
        Ok(facts)
    }

    /// Policy facts for one version, independent of review state (stack
    /// depth is 0). The per-path diff facts are cached as derived data.
    pub async fn version_facts(&self, version: &ChangeVersionHash) -> Result<ChangeFacts> {
        let cv = self.derive.change(version).await?;
        let paths = match self
            .derive
            .get_keyed::<CachedPaths>(PATH_FACTS_KIND, version)
            .await?
        {
            Some(cached) => cached.paths,
            None => {
                let paths = self.path_facts(&cv, version).await?;
                self.derive
                    .put_keyed(
                        PATH_FACTS_KIND,
                        version,
                        &CachedPaths {
                            paths: paths.clone(),
                        },
                    )
                    .await?;
                paths
            }
        };
        let mut facts =
            ChangeFacts::new(cv.author.clone(), paths).with_provenance(cv.provenance.clone());
        if let Some(intent_id) = cv.intent {
            if let Some(intent) = self.repo.get_intent(&intent_id).await? {
                facts = facts.with_intent(intent.kind, intent.risk_override);
            }
        }
        Ok(facts)
    }

    /// Review-side facts for the submit policy.
    pub fn submit_facts(&self, state: &ReviewState, change: &ChangeFacts) -> SubmitFacts {
        SubmitFacts::from_review(state, change)
    }

    /// Classify and record risk on the state.
    pub(crate) fn apply_risk(&self, state: &mut ReviewState, facts: &ChangeFacts) {
        let assessment = classify(facts, &self.risk);
        state.risk = assessment.class;
        state.risk_reasons = assessment.reasons;
    }

    async fn path_facts(
        &self,
        cv: &ChangeVersion,
        version: &ChangeVersionHash,
    ) -> Result<Vec<PathFacts>> {
        let root = self.derive.get::<RootTree>(version).await?;
        let parent_root = match cv.first_parent() {
            Some(p) => Some(self.derive.get::<RootTree>(&p).await?),
            None => None,
        };
        let entries = diff_trees(self.derive.trees(), parent_root, Some(root)).await?;
        let conflicted: BTreeSet<_> = cv.conflicted_paths().cloned().collect();
        let mut out = Vec::with_capacity(entries.len());
        let mut seen = BTreeSet::new();
        for entry in entries {
            seen.insert(entry.path.clone());
            let mut facts = self.entry_facts(&entry).await?;
            facts.conflict = conflicted.contains(&entry.path);
            out.push(facts);
        }
        // A conflict whose first term equals the parent leaves no tree
        // difference, but the path is still in conflict.
        for path in conflicted {
            if !seen.contains(&path) {
                out.push(PathFacts::conflicted(path));
            }
        }
        Ok(out)
    }

    async fn entry_facts(&self, entry: &TreeDiffEntry) -> Result<PathFacts> {
        let base = if entry.is_added() {
            PathFacts::added(entry.path.clone())
        } else if entry.is_deleted() {
            PathFacts::deleted(entry.path.clone())
        } else {
            PathFacts::modified(entry.path.clone())
        };
        let before = self.text_of(entry.before.as_ref().map(|(c, _)| c)).await?;
        let after = self.text_of(entry.after.as_ref().map(|(c, _)| c)).await?;
        Ok(match (before, after) {
            (Some(b), Some(a)) => {
                let (added, removed) = line_delta(&b, &a);
                base.with_lines(added, removed)
                    .with_cosmetic_only(cosmetic_only(&b, &a))
            }
            (None, Some(a)) if entry.is_added() => base.with_lines(line_count(&a), 0),
            (Some(b), None) if entry.is_deleted() => base.with_lines(0, line_count(&b)),
            _ => base,
        })
    }

    /// The content when it is text under the size cap; `None` for absent,
    /// oversized or binary content.
    pub(crate) async fn text_of(&self, content: Option<&ContentRef>) -> Result<Option<Bytes>> {
        let Some(content) = content else {
            return Ok(None);
        };
        if content.size() > MAX_DIFF_BYTES {
            return Ok(None);
        }
        let bytes = read_content(self.repo.objects().as_ref(), content).await?;
        Ok((!is_binary(&bytes)).then_some(bytes))
    }
}

/// Lines added and removed between two texts.
pub fn line_delta(before: &[u8], after: &[u8]) -> (u64, u64) {
    diff_lines(before, after).iter().fold((0, 0), |(a, r), h| {
        (a + h.added() as u64, r + h.removed() as u64)
    })
}

fn line_count(text: &[u8]) -> u64 {
    rill_core::diff::split_lines(text).count() as u64
}

/// Whether two texts differ only in whitespace and `//` or `#` comment
/// lines. A conservative approximation: a string literal containing `#`
/// at the start of a line would be missed, but risk classification only
/// uses this to lower the baseline, and sensitive-path floors still apply.
pub fn cosmetic_only(before: &[u8], after: &[u8]) -> bool {
    normalize(before) == normalize(after)
}

fn normalize(text: &[u8]) -> Vec<Vec<u8>> {
    rill_core::diff::split_lines(text)
        .map(|line| {
            line.iter()
                .copied()
                .filter(|b| !b.is_ascii_whitespace())
                .collect::<Vec<u8>>()
        })
        .filter(|line| !line.is_empty() && !line.starts_with(b"//") && !line.starts_with(b"#"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosmetic_detection() {
        assert!(cosmetic_only(b"fn a() {}\n", b"// doc\nfn a()   {}\n"));
        assert!(cosmetic_only(b"x = 1\n", b"# note\nx=1\n\n"));
        assert!(!cosmetic_only(b"fn a() {}\n", b"fn b() {}\n"));
        assert!(!cosmetic_only(b"a\n", b""));
    }

    #[test]
    fn line_deltas() {
        assert_eq!(line_delta(b"a\nb\n", b"a\nc\nd\n"), (2, 1));
        assert_eq!(line_delta(b"", b"x\n"), (1, 0));
    }
}
