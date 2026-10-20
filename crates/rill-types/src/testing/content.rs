use proptest::prelude::*;

use crate::{ContentRef, ObjectHash, INLINE_THRESHOLD};

/// Any object hash.
pub fn object_hash() -> impl Strategy<Value = ObjectHash> {
    any::<[u8; 32]>().prop_map(ObjectHash::from_bytes)
}

/// A content reference of any representation.
pub fn content_ref() -> impl Strategy<Value = ContentRef> {
    prop_oneof![
        prop::collection::vec(any::<u8>(), 0..=INLINE_THRESHOLD).prop_map(ContentRef::inline),
        (object_hash(), 0u64..1 << 20).prop_map(|(hash, size)| ContentRef::Blob { hash, size }),
        (object_hash(), 1u64 << 20..1 << 34)
            .prop_map(|(hash, size)| ContentRef::Chunked { hash, size }),
    ]
}
