//! Signing audit roots, with a key derived from a token key like grants.

use p256::ecdsa::signature::{Signer as _, Verifier as _};
use p256::ecdsa::{Signature, SigningKey, VerifyingKey};
use rill_types::audit::{AuditEntry, AuditRoot};

use crate::error::Result;

const DERIVATION: &str = "rill 2026 audit root signing key";

/// Signs audit roots.
#[derive(Clone)]
pub struct AuditSigner {
    key_id: u32,
    key: SigningKey,
}

impl std::fmt::Debug for AuditSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditSigner")
            .field("key_id", &self.key_id)
            .finish_non_exhaustive()
    }
}

impl AuditSigner {
    /// The audit key of token key `key_id` with secret `token_secret`.
    pub fn derive(key_id: u32, token_secret: &[u8]) -> Result<Self> {
        Ok(AuditSigner {
            key_id,
            key: crate::signing::derive_key(DERIVATION, token_secret)?,
        })
    }

    pub fn key_id(&self) -> u32 {
        self.key_id
    }

    /// The compressed public key.
    pub fn public_key(&self) -> Vec<u8> {
        self.key
            .verifying_key()
            .to_encoded_point(true)
            .as_bytes()
            .to_vec()
    }

    /// Fill in `root`'s key and signature.
    pub fn sign(&self, mut root: AuditRoot) -> AuditRoot {
        root.key_id = self.key_id;
        root.public_key = self.public_key();
        let signature: Signature = self.key.sign(&root.signed_bytes());
        let signature = signature.normalize_s().unwrap_or(signature);
        root.signature = signature.to_bytes().to_vec();
        root
    }
}

/// Whether `root`'s signature is valid under the public key it names. The
/// caller checks that key is one the instance published.
pub fn root_signature_holds(root: &AuditRoot) -> bool {
    let Ok(key) = VerifyingKey::from_sec1_bytes(&root.public_key) else {
        return false;
    };
    let Ok(signature) = Signature::from_slice(&root.signature) else {
        return false;
    };
    signature.normalize_s().is_none() && key.verify(&root.signed_bytes(), &signature).is_ok()
}

/// What a verified chain holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainReport {
    pub entries: u64,
    pub roots: usize,
    /// The last entry a root seals; entries after it are chained but not
    /// yet sealed.
    pub sealed_through: u64,
}

/// Where and why a chain fails verification.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("audit chain broken at entry {seq}: {reason}")]
pub struct ChainBreak {
    pub seq: u64,
    pub reason: String,
}

/// Check `entries` (an org's whole chain, in order) and `roots` against
/// each other and against `published` audit keys (by key id): every link,
/// every root's head and signature, and roots covering consecutive ranges.
pub fn verify_chain(
    entries: &[AuditEntry],
    roots: &[AuditRoot],
    published: &std::collections::HashMap<u32, Vec<u8>>,
) -> std::result::Result<ChainReport, ChainBreak> {
    let broken = |seq: u64, reason: &str| ChainBreak {
        seq,
        reason: reason.to_string(),
    };
    let mut hashes = Vec::with_capacity(entries.len());
    let mut prev = [0u8; 32];
    for (i, entry) in entries.iter().enumerate() {
        let expected = i as u64 + 1;
        if entry.seq != expected {
            return Err(broken(expected, "an entry is missing"));
        }
        if entry.prev != prev {
            return Err(broken(
                entry.seq,
                "the entry before it was changed or removed",
            ));
        }
        prev = entry.hash();
        hashes.push(prev);
    }
    let mut next_first = 1;
    for root in roots {
        if root.first_seq != next_first {
            return Err(broken(
                root.first_seq,
                "seals do not cover the chain in order",
            ));
        }
        let Some(head) = hashes.get((root.last_seq as usize).wrapping_sub(1)) else {
            return Err(broken(
                root.last_seq,
                "a seal covers entries that are not there",
            ));
        };
        if *head != root.head {
            return Err(broken(root.last_seq, "a sealed entry was changed"));
        }
        if !root_signature_holds(root) {
            return Err(broken(root.last_seq, "a seal's signature does not verify"));
        }
        if published.get(&root.key_id) != Some(&root.public_key) {
            return Err(broken(
                root.last_seq,
                "a seal is signed by a key the instance did not publish",
            ));
        }
        next_first = root.last_seq + 1;
    }
    Ok(ChainReport {
        entries: entries.len() as u64,
        roots: roots.len(),
        sealed_through: next_first - 1,
    })
}

#[cfg(test)]
mod tests {
    use rill_types::audit::{AuditHead, AuditOutcome, AuditRecord};
    use rill_types::{ObjectHash, ObjectKind, Principal, Timestamp};

    use super::*;

    fn chain(n: u64) -> Vec<AuditEntry> {
        let mut head = AuditHead::default();
        (0..n)
            .map(|i| {
                let entry = AuditEntry::after(
                    &head,
                    AuditRecord {
                        org: "acme".into(),
                        at: Timestamp::from_millis(i as i64),
                        actor: Principal::human("ann"),
                        action: "relation.add".into(),
                        target: "repo:web".into(),
                        outcome: AuditOutcome::Allowed,
                        detail: Default::default(),
                    },
                );
                head = AuditHead {
                    seq: entry.seq,
                    hash: entry.hash(),
                };
                entry
            })
            .collect()
    }

    #[test]
    fn verification_finds_every_kind_of_tampering() {
        let signer = AuditSigner::derive(1, &[3u8; 32]).unwrap();
        let published = std::collections::HashMap::from([(1, signer.public_key())]);
        let entries = chain(10);
        let seal = |first: u64, last: u64| {
            signer.sign(AuditRoot {
                org: "acme".into(),
                first_seq: first,
                last_seq: last,
                head: entries[last as usize - 1].hash(),
                export: ObjectHash::digest(ObjectKind::Blob, b"x"),
                sealed_at: Timestamp::from_millis(1),
                key_id: 0,
                public_key: Vec::new(),
                signature: Vec::new(),
            })
        };
        let roots = vec![seal(1, 4), seal(5, 8)];
        let report = verify_chain(&entries, &roots, &published).unwrap();
        assert_eq!(
            (report.entries, report.roots, report.sealed_through),
            (10, 2, 8)
        );

        let mut edited = entries.clone();
        edited[2].record.actor = Principal::human("mallory");
        assert_eq!(
            verify_chain(&edited, &roots, &published).unwrap_err().seq,
            4
        );

        let mut removed = entries.clone();
        removed.remove(5);
        assert_eq!(
            verify_chain(&removed, &roots, &published).unwrap_err().seq,
            6
        );

        // Rewriting an entry and every link after it still breaks the seal.
        let mut rewritten = entries.clone();
        rewritten[1].record.target = "repo:other".into();
        for i in 2..rewritten.len() {
            rewritten[i].prev = rewritten[i - 1].hash();
        }
        assert_eq!(
            verify_chain(&rewritten, &roots, &published)
                .unwrap_err()
                .seq,
            4
        );

        let stranger = AuditSigner::derive(1, &[9u8; 32]).unwrap();
        let forged = vec![stranger.sign(roots[0].clone())];
        assert!(verify_chain(&entries, &forged, &published).is_err());
        assert!(
            verify_chain(&entries, &[roots[1].clone()], &published).is_err(),
            "gaps"
        );
    }

    #[test]
    fn a_root_verifies_until_any_field_changes() {
        let signer = AuditSigner::derive(3, &[5u8; 32]).unwrap();
        let root = signer.sign(AuditRoot {
            org: "acme".into(),
            first_seq: 1,
            last_seq: 9,
            head: [1; 32],
            export: ObjectHash::digest(ObjectKind::Blob, b"lines"),
            sealed_at: Timestamp::from_millis(7),
            key_id: 0,
            public_key: Vec::new(),
            signature: Vec::new(),
        });
        assert!(root_signature_holds(&root));
        assert_eq!(root.key_id, 3);
        let mut moved = root.clone();
        moved.last_seq = 8;
        assert!(!root_signature_holds(&moved));
        let mut other_org = root.clone();
        other_org.org = "globex".into();
        assert!(!root_signature_holds(&other_org));
        assert_ne!(
            signer.public_key(),
            crate::grants::GrantSigner::derive(3, &[5u8; 32])
                .unwrap()
                .public_key(),
            "audit and grant keys differ"
        );
    }
}
