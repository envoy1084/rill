//! Packing and unpacking tuple keys.

use bytes::Bytes;
use foundationdb_tuple::{pack, unpack, TuplePack, TupleUnpack};

use crate::error::{Error, Result};

pub(super) fn key<T: TuplePack>(t: &T) -> Bytes {
    Bytes::from(pack(t))
}

/// Decode the tuple that follows `prefix` in `key`.
pub fn tail<'de, T: TupleUnpack<'de>>(prefix: &[u8], key: &'de [u8]) -> Result<T> {
    let rest = key
        .strip_prefix(prefix)
        .ok_or_else(|| Error::Codec("key outside its table".into()))?;
    unpack(rest).map_err(|e| Error::Codec(format!("malformed key: {e}")))
}
