use proptest::prelude::*;

use crate::RepoPathBuf;

/// A valid path component, including dots, spaces, mixed case and
/// non-ASCII letters, which are the cases that break naive path handling.
pub fn component() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-z][a-z0-9_]{0,7}",
        "[A-Za-z]{1,4}\\.[a-z]{1,3}",
        "\\.[a-z]{1,6}",
        "[a-z]{1,3} [a-z]{1,3}",
        "(é|ü|日本|Ω)[a-z]{0,3}",
    ]
    .prop_filter("not a relative component", |c| c != "." && c != "..")
}

/// A component that `RepoPathBuf` must reject.
pub fn invalid_component() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just(".".to_string()),
        Just("..".to_string()),
        "[a-z]{0,3}\\\\[a-z]{0,3}",
        "[a-z]{0,3}\0[a-z]{0,3}",
        "[a-z]{0,3}[\x01-\x1f\x7f][a-z]{0,3}",
        "[a-z]{256,300}",
    ]
}

/// A valid path of one to four components.
pub fn path() -> impl Strategy<Value = RepoPathBuf> {
    prop::collection::vec(component(), 1..=4)
        .prop_map(|parts| RepoPathBuf::new(parts.join("/")).expect("components are valid"))
}
