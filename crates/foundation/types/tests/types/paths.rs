//! Repository path validation.

use proptest::prelude::*;
use rill_types::testing::{component, invalid_component, path};
use rill_types::RepoPathBuf;
use unicode_normalization::UnicodeNormalization;

proptest! {
    #[test]
    fn valid_paths_roundtrip_through_text(p in path()) {
        prop_assert_eq!(RepoPathBuf::new(p.as_str()).unwrap(), p);
    }

    #[test]
    fn any_invalid_component_rejects_the_path(
        before in prop::collection::vec(component(), 0..3),
        bad in invalid_component(),
        after in prop::collection::vec(component(), 0..3),
    ) {
        // Leading and trailing slashes are trimmed, so an empty component
        // is only invalid between two real ones.
        prop_assume!(!bad.is_empty() || (!before.is_empty() && !after.is_empty()));
        let mut parts = before;
        parts.push(bad);
        parts.extend(after);
        prop_assert!(RepoPathBuf::new(parts.join("/")).is_err());
    }

    #[test]
    fn decomposed_spellings_name_the_same_path(p in path()) {
        let decomposed: String = p.as_str().nfd().collect();
        prop_assert_eq!(RepoPathBuf::new(decomposed).unwrap(), p);
    }
}

#[test]
fn paths_longer_than_the_limit_are_rejected() {
    let component = "a".repeat(200);
    let long = vec![component.as_str(); 21].join("/");
    assert!(long.len() > 4096);
    assert!(RepoPathBuf::new(&long).is_err());
    let mut built = RepoPathBuf::root();
    let pushed: Result<Vec<()>, _> = (0..21).map(|_| built.push(&component)).collect();
    assert!(pushed.is_err());
}
