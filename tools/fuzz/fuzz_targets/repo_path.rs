//! Untrusted path strings: validation never panics, and an accepted path
//! has no relative or empty components and round-trips through text.

#![no_main]

use libfuzzer_sys::fuzz_target;
use rill_types::RepoPathBuf;

fuzz_target!(|data: &str| {
    if let Ok(path) = RepoPathBuf::new(data) {
        let text = path.as_str();
        if !text.is_empty() {
            assert!(text
                .split('/')
                .all(|c| !c.is_empty() && c != "." && c != ".." && !c.contains('\\') && !c.contains('\0')));
        }
        assert_eq!(RepoPathBuf::new(text).unwrap(), path);
    }
});
