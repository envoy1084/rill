//! Conversions between `rill.v1` wire messages and `rill-types` values.
//!
//! Every wire message that mirrors a `rill-types` type gets `From<&T>` and
//! `From<T>` into the message and `TryFrom<Message>` back; decoding fails
//! with a [`ConvertError`](crate::ConvertError) naming the offending field.
//! Enums with an `UNSPECIFIED` zero value are rejected on decode, since no
//! `rill-types` enum has a default.

mod change;
mod content;
mod intent;
mod primitives;
mod principal;
mod refs;
mod review;
mod tree;
mod workspace;

#[cfg(test)]
mod roundtrip_tests;

pub use content::{
    file_kind_from_proto, file_state_from_proto, file_state_to_proto, merge_terms_from_proto,
    merge_terms_to_proto,
};
pub use intent::{intent_kind_from_proto, intent_status_from_proto, risk_from_proto};
pub use primitives::{
    enum_from_proto, hash_from_proto, hash_to_proto, hashes_from_proto, id_from_proto, id_to_proto,
    ids_from_proto, opt_hash_from_proto, opt_id_from_proto, path_from_proto, ref_name_from_proto,
    required, timestamp_from_proto, timestamp_to_proto, trunk_seq_from_proto, trunk_seq_to_proto,
    RillId,
};
pub use review::{comment_category_from_proto, confidence_from_proto, review_status_from_proto};
