//! Canonical binary encoding.
//!
//! Every immutable object is encoded as a one-byte format version followed by
//! a postcard-serialized body. Postcard is deterministic for a given type
//! (no map reordering, no whitespace), and every map in the object model is a
//! `BTreeMap`, so two equal values always produce identical bytes and
//! therefore identical hashes.
//!
//! The format version lets a type evolve: a decoder refuses versions it does
//! not know instead of misreading them.

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{Error, Result};
use crate::hash::{ObjectHash, ObjectKind};

/// Format version of a type's encoding.
pub type FormatVersion = u8;

/// Types with a canonical encoding and a content hash.
pub trait Encodable: Serialize + DeserializeOwned + Sized {
    const KIND: ObjectKind;
    const VERSION: FormatVersion;
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

pub fn encode<T: Encodable>(value: &T) -> Vec<u8> {
    let mut out = Vec::with_capacity(64);
    out.push(T::VERSION);
    postcard::to_extend(value, out).expect("postcard serialization of in-memory value cannot fail")
}

pub fn decode<T: Encodable>(bytes: &[u8]) -> Result<T> {
    let (&version, body) = bytes
        .split_first()
        .ok_or_else(|| Error::Decode(format!("{}: empty input", T::NAME)))?;
    if version != T::VERSION {
        return Err(Error::UnsupportedFormat {
            kind: T::NAME,
            found: version,
            expected: T::VERSION,
        });
    }
    postcard::from_bytes(body).map_err(|e| Error::Decode(format!("{}: {e}", T::NAME)))
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
        const VERSION: FormatVersion = 7;
        const NAME: &'static str = "sample";
    }

    #[test]
    fn roundtrip_and_version_check() {
        let s = Sample {
            a: 1,
            b: "x".into(),
        };
        let bytes = s.encode();
        assert_eq!(bytes[0], 7);
        assert_eq!(Sample::decode(&bytes).unwrap(), s);
        let mut bad = bytes.clone();
        bad[0] = 8;
        assert!(matches!(
            Sample::decode(&bad),
            Err(Error::UnsupportedFormat { found: 8, .. })
        ));
    }

    #[test]
    fn hash_is_stable() {
        let s = Sample {
            a: 1,
            b: "x".into(),
        };
        assert_eq!(
            s.hash(),
            Sample {
                a: 1,
                b: "x".into()
            }
            .hash()
        );
    }
}
