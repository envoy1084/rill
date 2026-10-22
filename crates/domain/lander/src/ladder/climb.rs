//! The conflict ladder at landing time. The rebase has already tried the
//! first rungs (the merge algebra, then the structural merger); what still
//! conflicts goes to the merge drivers the head declares, then to a
//! resolver agent when one is configured, then back to the owner.

use std::collections::BTreeMap;

use bytes::Bytes;
use rill_core::diff::is_binary;
use rill_core::merge::state::to_change;
use rill_core::{read_content, store_content, RootTree};
use rill_types::objects::change::FileState;
use rill_types::{ChangeVersion, FileChange, FileKind, Merge, RepoPathBuf, RiskClass};

use super::drivers::union;
use super::rules::{DriverKind, MergeRules, Regeneration};
use crate::error::Result;
use crate::land::Lander;
use crate::traits::{RegenerateRequest, Regenerated};

/// The extra field naming the files merge drivers resolved, by driver:
/// `changelog: CHANGELOG.md; cargo: Cargo.lock`.
pub const MERGE_DRIVERS_EXTRA: &str = "merge_drivers";

/// Where a rebased version ended on the ladder.
#[derive(Debug)]
pub enum Climb {
    /// Nothing conflicts any more.
    Resolved(Box<ChangeVersion>),
    Unresolved(Unresolved),
}

/// Conflicts the drivers left.
#[derive(Debug, Clone)]
pub struct Unresolved {
    /// Every conflicted path of the rebase.
    pub paths: Vec<RepoPathBuf>,
    /// Why a driver failed; `None` when no driver applies.
    pub reason: Option<String>,
    /// A path is manual, so no agent may take the conflict either.
    pub manual: bool,
}

impl Lander {
    /// Resolve what `cv` (a rebase onto the head `rules` were read at)
    /// still conflicts at with the merge drivers, for a change of `risk`.
    /// All or nothing: one path no driver resolves leaves every conflict.
    pub(crate) async fn climb(
        &self,
        mut cv: ChangeVersion,
        rules: &MergeRules,
        risk: RiskClass,
    ) -> Result<Climb> {
        cv.extra.remove(MERGE_DRIVERS_EXTRA);
        let conflicted: Vec<RepoPathBuf> = cv.conflicted_paths().cloned().collect();
        if conflicted.is_empty() {
            return Ok(Climb::Resolved(Box::new(cv)));
        }
        let unresolved = |reason: Option<String>, manual| {
            Ok(Climb::Unresolved(Unresolved {
                paths: conflicted.clone(),
                reason,
                manual,
            }))
        };
        let mut drivers = Vec::with_capacity(conflicted.len());
        for path in &conflicted {
            if rules.is_manual(path.as_str()) {
                return unresolved(None, true);
            }
            match rules.driver_for(path.as_str(), risk) {
                Some(driver) => drivers.push((path, driver)),
                None => return unresolved(None, false),
            }
        }

        let mut used: BTreeMap<&str, Vec<&RepoPathBuf>> = BTreeMap::new();
        let mut regenerate: BTreeMap<&str, (&Regeneration, Vec<RepoPathBuf>)> = BTreeMap::new();
        let mut heads: BTreeMap<RepoPathBuf, FileState> = BTreeMap::new();
        for (path, driver) in drivers {
            let Some(FileChange::Conflict(terms)) = cv.file_changes.get(path).cloned() else {
                continue;
            };
            let head = terms.first().clone();
            let state = match &driver.kind {
                DriverKind::Union => match self.union_of(&terms).await? {
                    Some(state) => state,
                    None => {
                        return unresolved(
                            Some(format!(
                                "{path} cannot be merged by union: the base and both sides \
                                 must be text files"
                            )),
                            false,
                        )
                    }
                },
                DriverKind::Generated => head.clone(),
                DriverKind::Regenerate(regeneration) => {
                    regenerate
                        .entry(driver.label())
                        .or_insert((regeneration, Vec::new()))
                        .1
                        .push(path.clone());
                    heads.insert(path.clone(), head.clone());
                    head.clone()
                }
            };
            settle(&mut cv, path, state, &head);
            used.entry(driver.label()).or_default().push(path);
        }

        if !regenerate.is_empty() {
            let staged = self.derive().store_version(&cv).await?;
            let root = self.derive().get::<RootTree>(&staged).await?;
            for (name, (regeneration, outputs)) in regenerate {
                let request = RegenerateRequest {
                    driver: name.to_string(),
                    root,
                    version: staged,
                    command: regeneration.command.clone(),
                    inputs: regeneration.inputs.clone(),
                    outputs: outputs.clone(),
                    env: regeneration.env.clone(),
                    platform: regeneration.platform.clone(),
                    timeout: regeneration.timeout,
                };
                let files = match self.regenerator.regenerate(request).await? {
                    Regenerated::Files(files) => files,
                    Regenerated::Failed(why) => {
                        return unresolved(
                            Some(format!("regenerate driver {name:?} failed: {why}")),
                            false,
                        )
                    }
                };
                for path in &outputs {
                    let Some(bytes) = files.get(path) else {
                        return unresolved(
                            Some(format!("regenerate driver {name:?} did not write {path}")),
                            false,
                        );
                    };
                    let head = heads.remove(path).flatten();
                    let kind = head.as_ref().map_or(FileKind::Regular, |(_, k)| *k);
                    let content =
                        store_content(self.repo().objects().as_ref(), bytes.clone()).await?;
                    settle(&mut cv, path, Some((content, kind)), &head);
                }
            }
        }
        record(&mut cv, &used);
        Ok(Climb::Resolved(Box::new(cv)))
    }

    /// The union of a three-way text conflict, stored; `None` for
    /// anything else.
    async fn union_of(&self, terms: &Merge<FileState>) -> Result<Option<FileState>> {
        let [Some((ours, kind)), Some((base, _)), Some((theirs, _))] = terms.as_slice() else {
            return Ok(None);
        };
        if *kind == FileKind::Symlink {
            return Ok(None);
        }
        let objects = self.repo().objects();
        let mut texts = Vec::with_capacity(3);
        for content in [ours, base, theirs] {
            let bytes = read_content(objects.as_ref(), content).await?;
            if is_binary(&bytes) {
                return Ok(None);
            }
            texts.push(bytes);
        }
        let merged = union(&texts[1], &texts[0], &texts[2]);
        let content = store_content(objects.as_ref(), Bytes::from(merged)).await?;
        Ok(Some(Some((content, *kind))))
    }
}

/// Put `state` at `path`, or no entry when it is what the head has.
fn settle(cv: &mut ChangeVersion, path: &RepoPathBuf, state: FileState, head: &FileState) {
    if state == *head {
        cv.file_changes.remove(path);
    } else {
        cv.file_changes.insert(path.clone(), to_change(state));
    }
}

/// Record on `cv` which drivers resolved which files.
fn record(cv: &mut ChangeVersion, used: &BTreeMap<&str, Vec<&RepoPathBuf>>) {
    let mut text = used
        .iter()
        .map(|(name, paths)| {
            let paths: Vec<&str> = paths.iter().map(|p| p.as_str()).collect();
            format!("{name}: {}", paths.join(", "))
        })
        .collect::<Vec<_>>()
        .join("; ");
    let limit = rill_types::limits::MAX_EXTRA_VALUE_BYTES - 4;
    if text.len() > limit {
        let mut cut = limit;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push('…');
    }
    cv.extra
        .insert(MERGE_DRIVERS_EXTRA.into(), Bytes::from(text.into_bytes()));
}
