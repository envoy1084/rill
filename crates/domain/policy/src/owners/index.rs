//! Every `OWNERS.toml` of a tree, and the owner sets covering a path.

use std::collections::BTreeMap;
use std::time::Duration;

use rill_types::{RepoPath, RepoPathBuf};
use serde::{Deserialize, Serialize};

use super::file::{relative, Delegation, OwnersFile};
use crate::error::Result;

/// A group of principals of which `required` must approve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerSet {
    /// Stable within a tree: `<dir>#default` or `<dir>#rule<n>`.
    pub id: String,
    /// The directory whose file declares it.
    pub dir: RepoPathBuf,
    pub owners: Vec<String>,
    pub required: u32,
    pub sensitive: bool,
    pub sla: Option<Duration>,
    pub escalate_to: Option<String>,
    /// Delegations the declaring file makes.
    pub delegates: Vec<Delegation>,
}

/// The `OWNERS.toml` files of a tree, by directory. Serializable as the
/// files' text, so it can be cached as derived data and recompiled.
#[derive(Debug, Clone, Default)]
pub struct OwnersIndex {
    files: BTreeMap<RepoPathBuf, OwnersFile>,
    texts: BTreeMap<String, String>,
}

/// The cached form: directory to file text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnersTexts(pub BTreeMap<String, String>);

impl OwnersIndex {
    /// Compile the files' texts, keyed by directory. A file that does not
    /// parse is skipped with a warning: a bad file on trunk must not stop
    /// review, including of the change that fixes it.
    pub fn compile(texts: OwnersTexts) -> Result<Self> {
        let mut files = BTreeMap::new();
        for (dir, text) in &texts.0 {
            match OwnersFile::parse(text) {
                Ok(file) => {
                    files.insert(RepoPathBuf::new(dir)?, file);
                }
                Err(e) => {
                    tracing::warn!(dir = %dir, error = %e, "skipping an unreadable OWNERS.toml")
                }
            }
        }
        Ok(OwnersIndex {
            files,
            texts: texts.0,
        })
    }

    /// The texts it was compiled from.
    pub fn texts(&self) -> OwnersTexts {
        OwnersTexts(self.texts.clone())
    }

    /// Whether any directory has a file.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// The owner sets covering `path`: the nearest file's matching rules
    /// (or its default owners), plus its parents' while files inherit.
    pub fn sets_for(&self, path: &RepoPath) -> Vec<OwnerSet> {
        let mut out = Vec::new();
        let mut dir = Some(path.parent().to_owned_path());
        while let Some(d) = dir {
            let next = (!d.is_root()).then(|| d.parent().to_owned_path());
            if let Some(file) = self.files.get(&d) {
                out.extend(sets_of(&d, file, path));
                if !file.inherit {
                    break;
                }
            }
            dir = next;
        }
        out
    }

    /// The distinct owner sets covering any of `paths`, in id order.
    pub fn sets_for_all<'a>(&self, paths: impl IntoIterator<Item = &'a RepoPath>) -> Vec<OwnerSet> {
        let mut by_id = BTreeMap::new();
        for path in paths {
            for set in self.sets_for(path) {
                by_id.entry(set.id.clone()).or_insert(set);
            }
        }
        by_id.into_values().collect()
    }
}

fn sets_of(dir: &RepoPathBuf, file: &OwnersFile, path: &RepoPath) -> Vec<OwnerSet> {
    let rel = relative(dir, path).unwrap_or_default();
    let base = |id: String, owners: &[String]| OwnerSet {
        id,
        dir: dir.clone(),
        owners: owners.to_vec(),
        required: 1,
        sensitive: false,
        sla: file.sla,
        escalate_to: file.escalate_to.clone(),
        delegates: file.delegates.clone(),
    };
    let rules: Vec<OwnerSet> = file
        .rules_for(rel)
        .map(|(i, rule)| OwnerSet {
            required: rule.required,
            sensitive: rule.sensitive,
            sla: rule.sla.or(file.sla),
            escalate_to: rule
                .escalate_to
                .clone()
                .or_else(|| file.escalate_to.clone()),
            ..base(format!("{}#rule{i}", dir.as_str()), &rule.owners)
        })
        .collect();
    if !rules.is_empty() {
        return rules;
    }
    if file.owners.is_empty() {
        return Vec::new();
    }
    vec![base(format!("{}#default", dir.as_str()), &file.owners)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> RepoPathBuf {
        RepoPathBuf::new(s).unwrap()
    }

    fn index(files: &[(&str, &str)]) -> OwnersIndex {
        OwnersIndex::compile(OwnersTexts(
            files
                .iter()
                .map(|(d, t)| (d.to_string(), t.to_string()))
                .collect(),
        ))
        .unwrap()
    }

    #[test]
    fn the_nearest_file_decides_unless_it_inherits() {
        let idx = index(&[
            ("", "owners = [\"team:platform\"]"),
            ("services/gateway", "owners = [\"team:gateway\"]\n[[rule]]\npaths = [\"auth/**\"]\nowners = [\"team:security\"]\nrequired = 2"),
            ("services/billing", "owners = [\"team:billing\"]\ninherit = true"),
        ]);
        let ids = |path: &str| {
            idx.sets_for(&p(path))
                .into_iter()
                .map(|s| s.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids("docs/a.md"), ["#default"]);
        assert_eq!(
            ids("services/gateway/main.rs"),
            ["services/gateway#default"]
        );
        assert_eq!(
            ids("services/gateway/auth/login.rs"),
            ["services/gateway#rule0"]
        );
        assert_eq!(
            ids("services/billing/x/invoice.rs"),
            ["services/billing#default", "#default"],
            "an inheriting file adds its parent's sets"
        );
        let all = idx.sets_for_all([
            p("services/gateway/auth/a").as_path(),
            p("docs/x").as_path(),
        ]);
        assert_eq!(all.len(), 2);
        assert_eq!(
            all.iter()
                .find(|s| s.id.ends_with("rule0"))
                .unwrap()
                .required,
            2
        );
    }

    #[test]
    fn an_unreadable_file_is_skipped_not_fatal() {
        let idx = index(&[("", "owners = [\"team:a\"]"), ("x", "owners = 3")]);
        assert_eq!(
            idx.sets_for(&p("x/y")).len(),
            1,
            "falls back to the root file"
        );
        assert_eq!(idx.texts().0.len(), 2, "the text is kept as written");
    }
}
