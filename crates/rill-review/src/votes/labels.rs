//! The set of vote labels configured on a repository.

use std::collections::BTreeMap;

use rill_types::Label;
use serde::{Deserialize, Serialize};

/// Labels keyed by name. Serialized as part of a repository's review
/// configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LabelSet {
    labels: BTreeMap<String, Label>,
}

impl LabelSet {
    /// An empty set; nothing can be voted on.
    pub fn empty() -> Self {
        LabelSet {
            labels: BTreeMap::new(),
        }
    }

    /// Build a set from labels; a later label with the same name replaces
    /// an earlier one.
    pub fn from_labels(labels: impl IntoIterator<Item = Label>) -> Self {
        LabelSet {
            labels: labels.into_iter().map(|l| (l.name.clone(), l)).collect(),
        }
    }

    /// Add or replace a label.
    pub fn insert(&mut self, label: Label) {
        self.labels.insert(label.name.clone(), label);
    }

    /// The label named `name`, if configured.
    pub fn get(&self, name: &str) -> Option<&Label> {
        self.labels.get(name)
    }

    /// Whether a label with this name exists.
    pub fn contains(&self, name: &str) -> bool {
        self.labels.contains_key(name)
    }

    /// All labels in name order.
    pub fn iter(&self) -> impl Iterator<Item = &Label> {
        self.labels.values()
    }

    /// Number of labels.
    pub fn len(&self) -> usize {
        self.labels.len()
    }

    /// Whether the set has no labels.
    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }
}

impl Default for LabelSet {
    /// The labels the default submit policy reasons about: `Review`,
    /// `Verified` and `Owner`.
    fn default() -> Self {
        LabelSet::from_labels([Label::review(), Label::verified(), Label::owner()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_three_labels() {
        let set = LabelSet::default();
        assert_eq!(set.len(), 3);
        assert!(set.contains("Review"));
        assert!(set.get("Owner").unwrap().max_requires_human);
        assert!(!set.contains("Nope"));
    }

    #[test]
    fn record_roundtrip() {
        let set = LabelSet::default();
        let bytes = rill_store::repo::encode_record(&set).unwrap();
        let back: LabelSet = rill_store::repo::decode_record(&bytes).unwrap();
        assert_eq!(back, set);
    }
}
