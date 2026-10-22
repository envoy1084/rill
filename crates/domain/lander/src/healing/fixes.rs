//! What a landed change says it fixes: trunk positions named in a
//! `Fixes: trunk@N` trailer of its message or in its provenance (`fixes`,
//! or `reverts` for a revert). Only explicit references count; a position
//! mentioned in passing does not.

use std::collections::BTreeSet;
use std::str::FromStr;

use rill_types::{ChangeVersion, TrunkSeq};

use crate::revert::REVERTS_KEY;

/// Provenance key listing the positions a change fixes, comma-separated.
pub const FIXES_KEY: &str = "fixes";

/// The trunk positions `cv` reverts or fixes.
pub fn fixes_of(cv: &ChangeVersion) -> BTreeSet<TrunkSeq> {
    let mut out = BTreeSet::new();
    for key in [REVERTS_KEY, FIXES_KEY] {
        if let Some(list) = cv.provenance.extra.get(key) {
            out.extend(positions(list));
        }
    }
    for line in cv.message.lines() {
        if let Some((key, value)) = line.split_once(':') {
            if key.trim().eq_ignore_ascii_case("fixes") {
                out.extend(positions(value));
            }
        }
    }
    out
}

/// Whether `cv` is a revert of some position.
pub fn is_revert(cv: &ChangeVersion) -> bool {
    cv.provenance.extra.contains_key(REVERTS_KEY)
}

/// `trunk@N` or `N` items of a comma or space separated list.
fn positions(list: &str) -> impl Iterator<Item = TrunkSeq> + '_ {
    list.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|item| !item.is_empty())
        .filter_map(|item| TrunkSeq::from_str(item).ok())
        .filter(|seq| seq.get() > 0)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rill_types::{ChangeId, Principal, Provenance, Timestamp};

    use super::*;

    fn cv(message: &str, extra: &[(&str, &str)]) -> ChangeVersion {
        ChangeVersion {
            change_id: ChangeId::new(),
            parents: vec![],
            predecessor: None,
            subtree_changes: BTreeMap::new(),
            file_changes: BTreeMap::new(),
            author: Principal::human("a"),
            provenance: Provenance {
                extra: extra
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
                ..Provenance::default()
            },
            intent: None,
            message: message.into(),
            created_at: Timestamp::from_millis(1),
            extra: BTreeMap::new(),
        }
    }

    fn seqs(list: &[u64]) -> BTreeSet<TrunkSeq> {
        list.iter().map(|s| TrunkSeq(*s)).collect()
    }

    #[test]
    fn trailers_and_provenance_name_fixes_and_mentions_do_not() {
        assert_eq!(
            fixes_of(&cv("Repair parsing\n\nFixes: trunk@12, trunk@14", &[])),
            seqs(&[12, 14])
        );
        assert_eq!(fixes_of(&cv("x\nfixes: 9", &[])), seqs(&[9]));
        assert_eq!(
            fixes_of(&cv("x", &[(FIXES_KEY, "trunk@3 4")])),
            seqs(&[3, 4])
        );
        assert_eq!(
            fixes_of(&cv("Revert trunk@7", &[(REVERTS_KEY, "7")])),
            seqs(&[7])
        );
        assert!(
            fixes_of(&cv("broke after trunk@5 landed", &[])).is_empty(),
            "a mention is not a claim"
        );
        assert!(fixes_of(&cv("Fixes: the parser", &[])).is_empty());
    }
}
