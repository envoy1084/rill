//! Targets a change affects: those whose hash it changed or that it added.

use super::hashes::TargetHashes;

/// Labels in `after` whose hash differs from `before`, or every label
/// when there is no `before`, in label order.
pub fn affected_targets(before: Option<&TargetHashes>, after: &TargetHashes) -> Vec<String> {
    after
        .iter()
        .filter(|(label, hash)| before.is_none_or(|b| b.get(*label) != Some(*hash)))
        .map(|(label, _)| label.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hashes(pairs: &[(&str, &str)]) -> TargetHashes {
        pairs
            .iter()
            .map(|(l, h)| (l.to_string(), h.to_string()))
            .collect()
    }

    #[test]
    fn changed_and_added_targets_are_affected_removed_ones_are_not() {
        let before = hashes(&[("//a", "1"), ("//b", "2"), ("//gone", "3")]);
        let after = hashes(&[("//a", "1"), ("//b", "9"), ("//new", "4")]);
        assert_eq!(affected_targets(Some(&before), &after), ["//b", "//new"]);
        assert_eq!(affected_targets(None, &after), ["//a", "//b", "//new"]);
        assert!(affected_targets(Some(&after), &after).is_empty());
    }
}
