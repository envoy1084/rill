//! ECDSA P-256 signatures over object hashes.
//!
//! A signature covers `"rill:sig:" || hash bytes`, so a signature over an
//! object hash can never be replayed as a signature over arbitrary data
//! signed with the same key in another protocol. Public keys are 33-byte
//! compressed SEC1 points and signatures 64 bytes (`r || s`, low-S).

use bytes::Bytes;
use p256::ecdsa::signature::{Signer as _, Verifier as _};
use p256::ecdsa::{Signature as EcdsaSignature, SigningKey, VerifyingKey};
use rill_types::{ObjectHash, Principal, Signature, Timestamp};

use crate::error::{Error, Result};

const DOMAIN: &[u8] = b"rill:sig:";

/// A P-256 signing key derived from a token key's secret for `context`, so
/// it is wrapped by the same key service and rotates with that key.
pub(crate) fn derive_key(context: &str, secret: &[u8]) -> Result<SigningKey> {
    let mut input = secret.to_vec();
    // A derived string is a valid scalar except with negligible
    // probability; extend the input and derive again then.
    for _ in 0..8 {
        if let Ok(key) = SigningKey::from_slice(&blake3::derive_key(context, &input)) {
            return Ok(key);
        }
        input.push(0);
    }
    Err(Error::InvalidKey(format!(
        "no {context} derives from this secret"
    )))
}

/// Length of a compressed public key.
pub const PUBLIC_KEY_LEN: usize = 33;

fn message(hash: &ObjectHash) -> Vec<u8> {
    let mut m = Vec::with_capacity(DOMAIN.len() + ObjectHash::LEN);
    m.extend_from_slice(DOMAIN);
    m.extend_from_slice(hash.as_bytes());
    m
}

/// A signing identity: a P-256 keypair.
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
        // A random 32-byte string is a valid scalar except with negligible
        // probability (zero or at least the group order); draw again then.
        loop {
            if let Ok(signer) = Signer::from_bytes(&rand::random::<[u8; 32]>()) {
                return signer;
            }
        }
    }

    /// Build from a 32-byte secret scalar.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        SigningKey::from_slice(bytes)
            .map(|key| Signer { key })
            .map_err(|e| Error::InvalidKey(format!("P-256 secret key: {e}")))
    }

    /// The 32-byte secret scalar. Treat as secret.
    pub fn secret_bytes(&self) -> [u8; 32] {
        self.key.to_bytes().into()
    }

    /// The compressed public key.
    pub fn public_key(&self) -> [u8; PUBLIC_KEY_LEN] {
        let point = self.key.verifying_key().to_encoded_point(true);
        let mut out = [0u8; PUBLIC_KEY_LEN];
        out.copy_from_slice(point.as_bytes());
        out
    }

    /// Sign an object hash on behalf of `principal`.
    pub fn sign(&self, hash: &ObjectHash, principal: &Principal) -> Signature {
        self.sign_at(hash, principal, Timestamp::now())
    }

    /// Sign with an explicit timestamp; the timestamp is informational and
    /// not covered by the signature.
    pub fn sign_at(&self, hash: &ObjectHash, principal: &Principal, at: Timestamp) -> Signature {
        let sig: EcdsaSignature = self.key.sign(&message(hash));
        let sig = sig.normalize_s().unwrap_or(sig);
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
/// binding is the caller's (or a key directory's) job. High-S signatures
/// are refused so each signature has one encoding.
pub fn verify(sig: &Signature, hash: &ObjectHash) -> bool {
    let Ok(key) = VerifyingKey::from_sec1_bytes(&sig.public_key) else {
        return false;
    };
    let Ok(signature) = EcdsaSignature::from_slice(&sig.signature) else {
        return false;
    };
    if signature.normalize_s().is_some() {
        return false;
    }
    key.verify(&message(hash), &signature).is_ok()
}

/// Hex encoding of a secret key for key files.
pub struct KeyFile;

impl KeyFile {
    /// Encode a signer's secret as 64 hex characters plus a trailing newline.
    pub fn encode(signer: &Signer) -> String {
        let mut s = hex::encode(signer.secret_bytes());
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
    pub fn encode_public(public_key: &[u8]) -> String {
        hex::encode(public_key)
    }

    /// Decode a hex compressed public key.
    pub fn decode_public(hex_str: &str) -> Result<[u8; PUBLIC_KEY_LEN]> {
        let bytes = hex::decode(hex_str.trim()).map_err(|e| Error::InvalidKey(e.to_string()))?;
        VerifyingKey::from_sec1_bytes(&bytes)
            .map_err(|e| Error::InvalidKey(format!("P-256 public key: {e}")))?;
        bytes
            .try_into()
            .map_err(|_| Error::InvalidKey("expected a 33-byte compressed public key".into()))
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
        assert_eq!(sig.public_key.len(), PUBLIC_KEY_LEN);
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
        assert_eq!(back.secret_bytes(), signer.secret_bytes());
        assert_eq!(back.public_key(), signer.public_key());
        assert!(KeyFile::decode("zz").is_err());
        assert!(Signer::from_bytes(&[0; 31]).is_err());
        assert!(Signer::from_bytes(&[0; 32]).is_err());
        let pk = KeyFile::decode_public(&KeyFile::encode_public(&signer.public_key())).unwrap();
        assert_eq!(pk, signer.public_key());
    }

    #[test]
    fn high_s_signatures_are_refused() {
        let signer = Signer::generate();
        let mut sig = signer.sign(&h(b"a"), &Principal::human("alice"));
        let low = EcdsaSignature::from_slice(&sig.signature).unwrap();
        let (r, s) = low.split_scalars();
        let high = EcdsaSignature::from_scalars(r, -*s).unwrap();
        sig.signature = Bytes::copy_from_slice(&high.to_bytes());
        assert!(!verify(&sig, &h(b"a")));
    }
}
