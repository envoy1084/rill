//! Scalar conversions: hashes, ids, paths, timestamps and enum helpers.

use bytes::Bytes;
use rill_types::{
    ChangeId, IntentId, ObjectHash, OperationId, PrincipalId, RefName, RepoId, RepoPathBuf,
    Timestamp, TrunkSeq, WorkspaceId,
};

use crate::error::{ConvertError, Result};

/// Encode an object hash as its 32 raw bytes.
pub fn hash_to_proto(hash: &ObjectHash) -> Bytes {
    Bytes::copy_from_slice(hash.as_bytes())
}

/// Decode a 32-byte field into an object hash, naming `field` on failure.
pub fn hash_from_proto(field: &'static str, bytes: &[u8]) -> Result<ObjectHash> {
    let arr: [u8; ObjectHash::LEN] = bytes.try_into().map_err(|_| ConvertError::BadLength {
        field,
        expected: ObjectHash::LEN,
        actual: bytes.len(),
    })?;
    Ok(ObjectHash::from_bytes(arr))
}

/// Decode an optional hash field; an empty `Some` is treated as absent.
pub fn opt_hash_from_proto(
    field: &'static str,
    bytes: Option<&Bytes>,
) -> Result<Option<ObjectHash>> {
    match bytes {
        None => Ok(None),
        Some(b) if b.is_empty() => Ok(None),
        Some(b) => hash_from_proto(field, b).map(Some),
    }
}

/// Decode a list of hashes, naming `field` on failure.
pub fn hashes_from_proto(field: &'static str, items: &[Bytes]) -> Result<Vec<ObjectHash>> {
    items.iter().map(|b| hash_from_proto(field, b)).collect()
}

/// A 128-bit record identifier from `rill-types`.
///
/// Implemented for every `define_id!` type so a single pair of helpers
/// covers change, intent, workspace, principal, repo and operation ids.
pub trait RillId: Sized {
    /// Length of the encoded id in bytes.
    const LEN: usize = 16;
    /// Build from raw bytes.
    fn from_id_bytes(bytes: [u8; 16]) -> Self;
    /// Borrow the raw bytes.
    fn id_bytes(&self) -> &[u8; 16];
}

macro_rules! impl_rill_id {
    ($($t:ty),*) => {
        $(
            impl RillId for $t {
                fn from_id_bytes(bytes: [u8; 16]) -> Self {
                    <$t>::from_bytes(bytes)
                }
                fn id_bytes(&self) -> &[u8; 16] {
                    self.as_bytes()
                }
            }
        )*
    };
}

impl_rill_id!(
    ChangeId,
    IntentId,
    WorkspaceId,
    PrincipalId,
    RepoId,
    OperationId
);

/// Encode an id as its 16 raw bytes.
pub fn id_to_proto<I: RillId>(id: &I) -> Bytes {
    Bytes::copy_from_slice(id.id_bytes())
}

/// Decode a 16-byte field into an id, naming `field` on failure.
pub fn id_from_proto<I: RillId>(field: &'static str, bytes: &[u8]) -> Result<I> {
    let arr: [u8; 16] = bytes.try_into().map_err(|_| ConvertError::BadLength {
        field,
        expected: I::LEN,
        actual: bytes.len(),
    })?;
    Ok(I::from_id_bytes(arr))
}

/// Decode an optional id field; an empty `Some` is treated as absent.
pub fn opt_id_from_proto<I: RillId>(
    field: &'static str,
    bytes: Option<&Bytes>,
) -> Result<Option<I>> {
    match bytes {
        None => Ok(None),
        Some(b) if b.is_empty() => Ok(None),
        Some(b) => id_from_proto(field, b).map(Some),
    }
}

/// Decode a list of ids, naming `field` on failure.
pub fn ids_from_proto<I: RillId>(field: &'static str, items: &[Bytes]) -> Result<Vec<I>> {
    items.iter().map(|b| id_from_proto(field, b)).collect()
}

/// Validate a repository path string.
pub fn path_from_proto(field: &'static str, path: &str) -> Result<RepoPathBuf> {
    RepoPathBuf::new(path).map_err(|e| ConvertError::invalid(field, e))
}

/// Validate a ref name string.
pub fn ref_name_from_proto(field: &'static str, name: &str) -> Result<RefName> {
    RefName::new(name).map_err(|e| ConvertError::invalid(field, e))
}

/// Timestamps travel as milliseconds since the epoch.
pub fn timestamp_to_proto(ts: Timestamp) -> i64 {
    ts.millis()
}

/// Timestamps travel as milliseconds since the epoch.
pub fn timestamp_from_proto(millis: i64) -> Timestamp {
    Timestamp::from_millis(millis)
}

/// Trunk sequence numbers travel as plain `uint64`.
pub fn trunk_seq_to_proto(seq: TrunkSeq) -> u64 {
    seq.get()
}

/// Trunk sequence numbers travel as plain `uint64`.
pub fn trunk_seq_from_proto(seq: u64) -> TrunkSeq {
    TrunkSeq(seq)
}

/// Decode a proto enum `i32`, rejecting unknown values and the zero
/// `UNSPECIFIED` sentinel, which is never a valid `rill-types` value.
pub fn enum_from_proto<E>(field: &'static str, value: i32) -> Result<E>
where
    E: TryFrom<i32>,
{
    if value == 0 {
        return Err(ConvertError::BadEnum { field, value });
    }
    E::try_from(value).map_err(|_| ConvertError::BadEnum { field, value })
}

/// Extract a required message field.
pub fn required<T>(field: &'static str, value: Option<T>) -> Result<T> {
    value.ok_or(ConvertError::Missing { field })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_roundtrip_and_length_check() {
        let h = ObjectHash::digest(rill_types::ObjectKind::Blob, b"x");
        assert_eq!(hash_from_proto("h", &hash_to_proto(&h)).unwrap(), h);
        assert_eq!(
            hash_from_proto("h", &[1, 2, 3]),
            Err(ConvertError::BadLength {
                field: "h",
                expected: 32,
                actual: 3
            })
        );
    }

    #[test]
    fn id_roundtrip_and_length_check() {
        let id = ChangeId::derived("t", "a");
        let decoded: ChangeId = id_from_proto("id", &id_to_proto(&id)).unwrap();
        assert_eq!(decoded, id);
        let err = id_from_proto::<WorkspaceId>("ws", &[0; 15]).unwrap_err();
        assert!(matches!(err, ConvertError::BadLength { field: "ws", .. }));
        assert_eq!(
            opt_id_from_proto::<IntentId>("i", Some(&Bytes::new())).unwrap(),
            None
        );
    }

    #[test]
    fn enum_rejects_unspecified() {
        let r: Result<crate::rill::v1::FileKind> = enum_from_proto("kind", 0);
        assert!(matches!(
            r,
            Err(ConvertError::BadEnum {
                field: "kind",
                value: 0
            })
        ));
        let r: Result<crate::rill::v1::FileKind> = enum_from_proto("kind", 99);
        assert!(r.is_err());
        let r: Result<crate::rill::v1::FileKind> = enum_from_proto("kind", 1);
        assert_eq!(r.unwrap(), crate::rill::v1::FileKind::Regular);
    }

    #[test]
    fn path_and_ref_errors_name_field() {
        let err = path_from_proto("path", "a/../b").unwrap_err();
        assert!(matches!(err, ConvertError::Invalid { field: "path", .. }));
        let err = ref_name_from_proto("name", "a b").unwrap_err();
        assert!(matches!(err, ConvertError::Invalid { field: "name", .. }));
    }
}
