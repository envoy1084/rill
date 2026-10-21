//! SCIM requests from a provisioning system: filters, resources and patch
//! operations never panic.

#![no_main]

use libfuzzer_sys::fuzz_target;
use rill_identity::scim::{patch_user, Filter, UserFields};

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        if let Ok(filter) = Filter::parse(text) {
            filter.matches(|_| Some(text.to_string()));
        }
    }
    if let Ok(body) = serde_json::from_slice::<serde_json::Value>(data) {
        let mut user = match UserFields::parse(&body) {
            Ok(user) => user,
            Err(_) => UserFields {
                user_name: "ann".into(),
                external_id: None,
                display_name: None,
                email: None,
                active: true,
            },
        };
        let _ = patch_user(&mut user, &body);
    }
});
