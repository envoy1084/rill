//! `POST /o/{repo}/batch` bodies: parsing the request never panics.

#![no_main]

use libfuzzer_sys::fuzz_target;
use rill_api::objects::batch::BatchItem;
use rill_api::objects::handlers::parse_ref;

fuzz_target!(|data: &[u8]| {
    if let Ok(items) = serde_json::from_slice::<Vec<BatchItem>>(data) {
        for item in items {
            let _ = parse_ref(&item.kind, &item.hash);
            if let Some(via) = item.via {
                let _ = parse_ref("manifest", &via);
            }
        }
    }
});
