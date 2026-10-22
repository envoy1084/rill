//! The content side of merge drivers, without storage.

use gix_merge::blob::builtin_driver::text::{Conflict, Labels, Options};

/// The union of two edits of `base`: where they conflict, `ours`'s lines
/// then `theirs`'s, as git's `merge=union` does.
pub fn union(base: &[u8], ours: &[u8], theirs: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut input = gix_imara_diff::InternedInput::default();
    gix_merge::blob::builtin_driver::text(
        &mut out,
        &mut input,
        Labels::default(),
        ours,
        base,
        theirs,
        Options {
            conflict: Conflict::ResolveWithUnion,
            ..Options::default()
        },
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_sides_appended_entries_are_kept_ours_first() {
        let base = b"[a]\nx = 1\n";
        let ours = b"[a]\nx = 1\n[b]\ny = 2\n";
        let theirs = b"[a]\nx = 1\n[c]\nz = 3\n";
        assert_eq!(
            union(base, ours, theirs),
            b"[a]\nx = 1\n[b]\ny = 2\n[c]\nz = 3\n"
        );
    }

    #[test]
    fn edits_that_do_not_conflict_merge_as_usual() {
        assert_eq!(
            union(b"a\nb\nc\nd\n", b"A\nb\nc\nd\n", b"a\nb\nc\nD\n"),
            b"A\nb\nc\nD\n"
        );
    }
}
