//! Encodings of the values `RepoStore` keeps in the metadata store.

use bytes::Bytes;
use rill_types::{ObjectHash, Record};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{Error, Result};

/// Encode a mutable record (not content-addressed) with its type tag.
pub fn encode_record<T: Record>(value: &T) -> Result<Bytes> {
    Ok(Bytes::from(rill_types::encode_record(value)))
}

/// Decode a record, refusing other record types and schemas.
pub fn decode_record<T: Record>(bytes: &[u8]) -> Result<T> {
    rill_types::decode_record(bytes).map_err(|e| Error::Codec(e.to_string()))
}

/// Encode a value that is not a [`Record`] under an explicit tag, usually
/// the storage kind it is kept under.
pub fn encode_tagged<T: Serialize + ?Sized>(tag: &str, value: &T) -> Result<Bytes> {
    Ok(Bytes::from(rill_types::encode_tagged(tag, 1, value)))
}

/// Decode a value written by [`encode_tagged`] with the same tag.
pub fn decode_tagged<T: DeserializeOwned>(tag: &str, bytes: &[u8]) -> Result<T> {
    rill_types::decode_tagged(tag, 1, bytes).map_err(|e| Error::Codec(e.to_string()))
}

pub(super) fn hash_value(hash: &ObjectHash) -> Bytes {
    Bytes::copy_from_slice(hash.as_bytes())
}

pub(super) fn parse_hash(bytes: &[u8]) -> Result<ObjectHash> {
    Ok(ObjectHash::from_slice(bytes)?)
}

pub(super) fn parse_seq(bytes: &[u8]) -> Result<u64> {
    let arr: [u8; 8] = bytes
        .try_into()
        .map_err(|_| Error::Codec("bad sequence number".into()))?;
    Ok(u64::from_be_bytes(arr))
}
