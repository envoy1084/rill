//! Repository and server configuration text: parsing never panics.
//! The first byte picks the format.

#![no_main]

use libfuzzer_sys::fuzz_target;
use rill_policy::owners::OwnersFile;
use rill_policy::secrets::SecretRules;
use rill_policy::SubmitPolicy;

fuzz_target!(|data: &[u8]| {
    let Some((which, rest)) = data.split_first() else {
        return;
    };
    let Ok(text) = std::str::from_utf8(rest) else {
        return;
    };
    match which % 4 {
        0 => drop(OwnersFile::parse(text)),
        1 => drop(SecretRules::parse(text)),
        2 => drop(SubmitPolicy::from_source(text)),
        _ => drop(rill_api::ServerConfig::from_toml(text)),
    }
});
