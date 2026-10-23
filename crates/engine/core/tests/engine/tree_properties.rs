//! Building trees from edits and diffing them agree on what changed.

use std::collections::BTreeMap;
use std::sync::Arc;

use proptest::prelude::*;
use rill_core::diff_trees;
use rill_core::tree::{Edit, TreeBuilder, TreeOps};
use rill_store::MemoryObjectStore;
use rill_types::{ContentRef, FileKind, RepoPathBuf};

/// Two-level paths whose directory and file names never collide, so every
/// map is a valid tree.
fn files() -> impl Strategy<Value = BTreeMap<String, u8>> {
    prop::collection::btree_map(
        ("d[0-3]", "f[0-5]").prop_map(|(d, f)| format!("{d}/{f}")),
        0u8..4,
        0..12,
    )
}

fn build(
    ops: &TreeOps,
    files: &BTreeMap<String, u8>,
    rt: &tokio::runtime::Runtime,
) -> rill_types::TreeHash {
    let mut builder = TreeBuilder::new();
    for (path, v) in files {
        let path = RepoPathBuf::new(path).unwrap();
        builder.edit(
            &path,
            Edit::Set(ContentRef::inline(vec![*v]), FileKind::Regular),
        );
    }
    rt.block_on(builder.build(ops, None)).unwrap()
}

proptest! {
    #[test]
    fn diff_reports_exactly_the_changed_paths(a in files(), b in files()) {
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let ops = TreeOps::new(Arc::new(MemoryObjectStore::new()), 64);
        let (ta, tb) = (build(&ops, &a, &rt), build(&ops, &b, &rt));

        let diff = rt.block_on(diff_trees(&ops, Some(ta), Some(tb))).unwrap();
        let got: Vec<String> = diff.iter().map(|e| e.path.to_string()).collect();
        let mut want: Vec<String> = a.keys().chain(b.keys())
            .filter(|p| a.get(*p) != b.get(*p))
            .cloned()
            .collect();
        want.sort();
        want.dedup();
        prop_assert_eq!(got, want);
        prop_assert_eq!(ta == tb, a == b);
    }
}
