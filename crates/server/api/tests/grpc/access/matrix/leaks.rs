//! The check that an answer holds nothing from the hidden directory.

use rill_types::{ObjectHash, ObjectKind};
use tonic::Code;

use super::super::fixture::*;
use super::super::raw::Outcome;

/// Nothing from `secret/` in an answer to someone who cannot read it.
pub(super) fn assert_no_leak(what: &str, outcome: &Outcome) {
    let secret_hash = ObjectHash::digest(ObjectKind::Blob, &secret_bytes());
    let body = &outcome.body[..];
    let contains = |needle: &[u8]| body.windows(needle.len()).any(|w| w == needle);
    assert!(!contains(CANARY.as_bytes()), "{what}: the canary leaked");
    if outcome.code == Code::Ok {
        assert!(!contains(b"secret"), "{what}: a hidden path leaked");
        assert!(
            !contains(secret_hash.as_bytes()),
            "{what}: a hidden hash leaked"
        );
    }
}
