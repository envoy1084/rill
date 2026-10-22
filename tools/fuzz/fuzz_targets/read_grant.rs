//! Untrusted read grants: verifying never panics and nothing but a grant
//! the key signed verifies.

#![no_main]

use libfuzzer_sys::fuzz_target;
use rill_policy::grants::{GrantKeys, GrantSigner};
use rill_types::Timestamp;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let signer = GrantSigner::derive(1, &[7u8; 32]).unwrap();
    let mut keys = GrantKeys::new();
    keys.insert(1, &signer.public_key(), None).unwrap();
    assert!(keys.verify(text, Timestamp::from_millis(0)).is_err());
});
