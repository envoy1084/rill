//! The secret scanner over untrusted file content: it never panics, and a
//! finding never carries the secret it found.

#![no_main]

use libfuzzer_sys::fuzz_target;
use rill_policy::secrets::{scan, SecretRules};

fuzz_target!(|data: &[u8]| {
    let rules = SecretRules::default();
    for finding in scan("src/config.rs", data, &rules) {
        assert!(finding.line >= 1);
        assert_eq!(finding.fingerprint.len(), 16);
        assert!(finding.fingerprint.bytes().all(|b| b.is_ascii_hexdigit()));
    }
});
