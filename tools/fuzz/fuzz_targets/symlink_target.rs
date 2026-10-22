//! Untrusted symlinks: an accepted target, resolved from the link's
//! directory, never climbs above the repository root.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use rill_types::limits::check_symlink_target;
use rill_types::RepoPathBuf;

#[derive(Arbitrary, Debug)]
struct Input<'a> {
    link: &'a str,
    target: &'a [u8],
}

fuzz_target!(|input: Input<'_>| {
    let Ok(link) = RepoPathBuf::new(input.link) else {
        return;
    };
    if link.as_str().is_empty() || check_symlink_target(&link, input.target).is_err() {
        return;
    }
    let target = std::str::from_utf8(input.target).expect("accepted targets are UTF-8");
    assert!(!target.starts_with('/'));
    let mut depth = link.as_str().split('/').count() as i64 - 1;
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => depth -= 1,
            _ => depth += 1,
        }
        assert!(depth >= 0, "{link} -> {target} escapes the repository");
    }
});
