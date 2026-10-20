//! Canonical encoding: identity depends on value, not on how it was built.

use proptest::prelude::*;
use rill_types::testing::change_version;
use rill_types::{ChangeVersion, Encodable};

proptest! {
    #[test]
    fn change_version_roundtrips_and_reencodes_identically(cv in change_version()) {
        let bytes = cv.encode();
        let back = ChangeVersion::decode(&bytes).unwrap();
        prop_assert_eq!(&back, &cv);
        prop_assert_eq!(back.encode(), bytes);
        prop_assert_eq!(back.hash(), cv.hash());
    }

    #[test]
    fn changing_the_message_changes_the_hash(cv in change_version(), suffix in "[a-z]{1,4}") {
        let mut other = cv.clone();
        other.message.push_str(&suffix);
        prop_assert_ne!(other.hash(), cv.hash());
    }
}
