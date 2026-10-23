//! Diff and patch agree for any content and any edit.

use proptest::prelude::*;
use rill_core::diff::{diff_lines_with_context, split_lines};
use rill_core::{apply_unified, to_unified};
use rill_types::testing::{edited, text};

fn base_and_edit() -> impl Strategy<Value = (Vec<u8>, Vec<u8>)> {
    text().prop_flat_map(|base| (Just(base.clone()), edited(base)))
}

proptest! {
    #[test]
    fn patch_reproduces_the_edit((old, new) in base_and_edit(), context in 0usize..4) {
        let patch = to_unified("f", &diff_lines_with_context(&old, &new, context));
        prop_assert_eq!(apply_unified(&old, &patch).unwrap(), new);
    }

    #[test]
    fn hunks_reconstruct_both_sides((old, new) in base_and_edit()) {
        let hunks = diff_lines_with_context(&old, &new, usize::MAX / 2);
        let (mut before, mut after) = (Vec::new(), Vec::new());
        for line in hunks.iter().flat_map(|h| &h.lines) {
            use rill_core::LineTag::*;
            match line.tag {
                Context => { before.extend_from_slice(&line.text); after.extend_from_slice(&line.text); }
                Removed => before.extend_from_slice(&line.text),
                Added => after.extend_from_slice(&line.text),
            }
        }
        if hunks.is_empty() {
            prop_assert_eq!(&old, &new);
        } else {
            prop_assert_eq!(before, old.clone());
            prop_assert_eq!(after, new);
        }
        prop_assert_eq!(split_lines(&old).map(<[u8]>::len).sum::<usize>(), old.len());
    }
}
