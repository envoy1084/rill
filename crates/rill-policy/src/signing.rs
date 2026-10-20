//! Ed25519 signatures over object hashes.
//!
//! A signature covers `"rill:sig:" || hash bytes`, so a signature over an
//! object hash can never be replayed as a signature over arbitrary data
//! signed with the same key in another protocol.

use bytes::Bytes;
use ed25519_dalek::{Signature as DalekSignature, Signer as _, SigningKey, VerifyingKey};
use rill_types::{ObjectHash, Principal, Signature, Timestamp};

use crate::error::{Error, Result};

const DOMAIN: &[u8] = b"rill:sig:";

fn message(hash: &ObjectHash) -> Vec<u8> {
    let mut m = Vec::with_capacity(DOMAIN.len() + ObjectHash::LEN);
    m.extend_from_slice(DOMAIN);
    m.extend_from_slice(hash.as_bytes());
    m
}

/// A signing identity: an Ed25519 keypair.
#[derive(Clone)]
pub struct Signer {
    key: SigningKey,
}

impl std::fmt::Debug for Signer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Signer")
            .field("public_key", &hex::encode(self.public_key()))
            .finish()
    }
}

impl Signer {
    /// Generate a fresh random keypair.
    pub fn generate() -> Self {
        // rand's thread RNG is a CSPRNG seeded from the OS, and a seed is all
        // an Ed25519 key needs; this avoids coupling to ed25519's rand_core.
        Signer::from_seed(&rand::random())
    }

    /// Build from a 32-byte seed.
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        Signer {
            key: SigningKey::from_bytes(seed),
        }
    }

    /// Build from seed bytes of any length; must be exactly 32.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let seed: [u8; 32] = bytes.try_into().map_err(|_| {
            Error::InvalidKey(format!("expected 32-byte seed, got {}", bytes.len()))
        })?;
        Ok(Self::from_seed(&seed))
    }

    /// The 32-byte seed. Treat as secret.
    pub fn seed(&self) -> [u8; 32] {
        self.key.to_bytes()
    }

    /// The 32-byte public key.
    pub fn public_key(&self) -> [u8; 32] {
        self.key.verifying_key().to_bytes()
    }

    /// Sign an object hash on behalf of `principal`.
    pub fn sign(&self, hash: &ObjectHash, principal: &Principal) -> Signature {
        self.sign_at(hash, principal, Timestamp::now())
    }

    /// Sign with an explicit timestamp; the timestamp is informational and
    /// not covered by the signature.
    pub fn sign_at(&self, hash: &ObjectHash, principal: &Principal, at: Timestamp) -> Signature {
        let sig = self.key.sign(&message(hash));
        Signature {
            signer: principal.clone(),
            public_key: Bytes::copy_from_slice(&self.public_key()),
            signature: Bytes::copy_from_slice(&sig.to_bytes()),
            signed_at: at,
        }
    }
}

/// Verify that `sig` is a valid signature over `hash` by the public key it
/// carries. Does not check that the key belongs to `sig.signer`; that
/// binding is the caller's (or a key directory's) job.
pub fn verify(sig: &Signature, hash: &ObjectHash) -> bool {
    let Ok(key_bytes) = <[u8; 32]>::try_from(sig.public_key.as_ref()) else {
        return false;
    };
    let Ok(key) = VerifyingKey::from_bytes(&key_bytes) else {
        return false;
    };
    let Ok(sig_bytes) = <[u8; 64]>::try_from(sig.signature.as_ref()) else {
        return false;
    };
    let signature = DalekSignature::from_bytes(&sig_bytes);
    key.verify_strict(&message(hash), &signature).is_ok()
}

/// Hex encoding of a keypair seed for key files.
pub struct KeyFile;

impl KeyFile {
    /// Encode a signer's seed as 64 hex characters plus a trailing newline.
    pub fn encode(signer: &Signer) -> String {
        let mut s = hex::encode(signer.seed());
        s.push('\n');
        s
    }

    /// Decode a key file produced by [`KeyFile::encode`]. Whitespace around
    /// the hex is ignored.
    pub fn decode(contents: &str) -> Result<Signer> {
        let bytes = hex::decode(contents.trim()).map_err(|e| Error::InvalidKey(e.to_string()))?;
        Signer::from_bytes(&bytes)
    }

    /// Encode a public key as hex.
    pub fn encode_public(public_key: &[u8; 32]) -> String {
        hex::encode(public_key)
    }

    /// Decode a hex public key.
    pub fn decode_public(hex_str: &str) -> Result<[u8; 32]> {
        let bytes = hex::decode(hex_str.trim()).map_err(|e| Error::InvalidKey(e.to_string()))?;
        bytes
            .try_into()
            .map_err(|_| Error::InvalidKey("expected 32-byte public key".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_types::ObjectKind;

    fn h(s: &[u8]) -> ObjectHash {
        ObjectHash::digest(ObjectKind::Blob, s)
    }

    #[test]
    fn sign_and_verify() {
        let signer = Signer::generate();
        let alice = Principal::human("alice");
        let sig = signer.sign(&h(b"a"), &alice);
        assert_eq!(sig.signer, alice);
        assert_eq!(sig.public_key.len(), 32);
        assert_eq!(sig.signature.len(), 64);
        assert!(verify(&sig, &h(b"a")));
        assert!(!verify(&sig, &h(b"b")));
    }

    #[test]
    fn tampered_signature_fails() {
        let signer = Signer::generate();
        let mut sig = signer.sign(&h(b"a"), &Principal::human("alice"));
        let mut bytes = sig.signature.to_vec();
        bytes[0] ^= 1;
        sig.signature = Bytes::from(bytes);
        assert!(!verify(&sig, &h(b"a")));
        sig.signature = Bytes::from_static(b"short");
        assert!(!verify(&sig, &h(b"a")));
    }

    #[test]
    fn wrong_key_fails() {
        let a = Signer::generate();
        let b = Signer::generate();
        let mut sig = a.sign(&h(b"a"), &Principal::human("alice"));
        sig.public_key = Bytes::copy_from_slice(&b.public_key());
        assert!(!verify(&sig, &h(b"a")));
    }

    #[test]
    fn key_file_roundtrip() {
        let signer = Signer::generate();
        let text = KeyFile::encode(&signer);
        assert_eq!(text.len(), 65);
        let back = KeyFile::decode(&text).unwrap();
        assert_eq!(back.seed(), signer.seed());
        assert_eq!(back.public_key(), signer.public_key());
        assert!(KeyFile::decode("zz").is_err());
        assert!(Signer::from_bytes(&[0; 31]).is_err());
        let pk = KeyFile::decode_public(&KeyFile::encode_public(&signer.public_key())).unwrap();
        assert_eq!(pk, signer.public_key());
    }
}
