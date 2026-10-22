//! Canonical binary encoding for objects and records.
//!
//! An object is `[kind u8][schema u16 BE][postcard body]`. The header is part
//! of the hashed bytes, so an object's identity covers its type and schema;
//! decoding checks both, so a tree can never be read as a change version.
//! Postcard is deterministic for a given type and every map in the model is
//! a `BTreeMap`, so equal values give identical bytes and identical hashes.
//!
//! A record (mutable, not hashed) is `[schema u16 BE][tag len u8][tag]
//! [postcard body]`. The tag names the record type, so reading the wrong
//! type is an error rather than garbage. Rill is pre-release: a schema
//! mismatch is an error, never an upgrade path.

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{Error, Result};
use crate::hash::{ObjectHash, ObjectKind};

/// Schema number of an encoded type.
pub type Schema = u16;

/// Types with a canonical encoding and a content hash.
pub trait Encodable: Serialize + DeserializeOwned + Sized {
    const KIND: ObjectKind;
    const SCHEMA: Schema;
    const NAME: &'static str;

    fn encode(&self) -> Vec<u8> {
        encode(self)
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        decode(bytes)
    }

    fn hash(&self) -> ObjectHash {
        ObjectHash::digest(Self::KIND, &self.encode())
    }
}

/// Mutable records stored by key, identified by a short type tag.
pub trait Record: Serialize + DeserializeOwned + Sized {
    const TAG: &'static str;
    const SCHEMA: Schema = 1;
}

pub fn encode<T: Encodable>(value: &T) -> Vec<u8> {
    let mut out = Vec::with_capacity(64);
    out.push(T::KIND as u8);
    out.extend_from_slice(&T::SCHEMA.to_be_bytes());
    postcard::to_extend(value, out).expect("postcard serialization of in-memory value cannot fail")
}

pub fn decode<T: Encodable>(bytes: &[u8]) -> Result<T> {
    let [kind, s0, s1, body @ ..] = bytes else {
        return Err(Error::Decode(format!("{}: truncated header", T::NAME)));
    };
    if *kind != T::KIND as u8 {
        return Err(Error::Decode(format!(
            "{}: object kind {kind}, expected {}",
            T::NAME,
            T::KIND as u8
        )));
    }
    let schema = Schema::from_be_bytes([*s0, *s1]);
    if schema != T::SCHEMA {
        return Err(Error::UnsupportedSchema {
            kind: T::NAME,
            found: schema,
            expected: T::SCHEMA,
        });
    }
    postcard::from_bytes(body).map_err(|e| Error::Decode(format!("{}: {e}", T::NAME)))
}

/// Encode a record of type `T`.
pub fn encode_record<T: Record>(value: &T) -> Vec<u8> {
    encode_tagged(T::TAG, T::SCHEMA, value)
}

/// Decode a record of type `T`, refusing other tags and schemas.
pub fn decode_record<T: Record>(bytes: &[u8]) -> Result<T> {
    decode_tagged(T::TAG, T::SCHEMA, bytes)
}

/// Encode `value` under an explicit tag, for values whose type is not a
/// [`Record`] (derived data is tagged with its `kind@version` key).
pub fn encode_tagged<T: Serialize + ?Sized>(tag: &str, schema: Schema, value: &T) -> Vec<u8> {
    let tag = tag.as_bytes();
    let len = u8::try_from(tag.len()).expect("record tags are short constants");
    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(&schema.to_be_bytes());
    out.push(len);
    out.extend_from_slice(tag);
    postcard::to_extend(value, out).expect("postcard serialization of in-memory value cannot fail")
}

/// Decode a value written by [`encode_tagged`] with the same tag and schema.
pub fn decode_tagged<T: DeserializeOwned>(tag: &str, schema: Schema, bytes: &[u8]) -> Result<T> {
    let [s0, s1, len, rest @ ..] = bytes else {
        return Err(Error::Decode(format!("{tag}: truncated record header")));
    };
    let (found_tag, body) = rest
        .split_at_checked(*len as usize)
        .ok_or_else(|| Error::Decode(format!("{tag}: truncated record tag")))?;
    if found_tag != tag.as_bytes() {
        return Err(Error::Decode(format!(
            "expected a {tag} record, found {}",
            String::from_utf8_lossy(found_tag)
        )));
    }
    let found = Schema::from_be_bytes([*s0, *s1]);
    if found != schema {
        return Err(Error::UnsupportedSchema {
            kind: "record",
            found,
            expected: schema,
        });
    }
    postcard::from_bytes(body).map_err(|e| Error::Decode(format!("{tag}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Sample {
        a: u32,
        b: String,
    }

    impl Encodable for Sample {
        const KIND: ObjectKind = ObjectKind::Derived;
        const SCHEMA: Schema = 7;
        const NAME: &'static str = "sample";
    }

    impl Record for Sample {
        const TAG: &'static str = "sample";
    }

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Other {
        a: u32,
        b: String,
    }

    impl Record for Other {
        const TAG: &'static str = "other";
    }

    fn sample() -> Sample {
        Sample {
            a: 1,
            b: "x".into(),
        }
    }

    #[test]
    fn objects_check_kind_and_schema() {
        let bytes = sample().encode();
        assert_eq!(&bytes[..3], &[ObjectKind::Derived as u8, 0, 7]);
        assert_eq!(Sample::decode(&bytes).unwrap(), sample());
        let mut wrong_schema = bytes.clone();
        wrong_schema[2] = 8;
        assert!(matches!(
            Sample::decode(&wrong_schema),
            Err(Error::UnsupportedSchema { found: 8, .. })
        ));
        let mut wrong_kind = bytes;
        wrong_kind[0] = ObjectKind::Tree as u8;
        assert!(Sample::decode(&wrong_kind).is_err());
    }

    #[test]
    fn records_refuse_another_type() {
        let bytes = encode_record(&sample());
        assert_eq!(decode_record::<Sample>(&bytes).unwrap(), sample());
        let err = decode_record::<Other>(&bytes).unwrap_err();
        assert!(err.to_string().contains("expected a other record"), "{err}");
    }
}
