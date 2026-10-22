//! Signing keys: which key verifies a token and how keys are encoded.

use rill_types::Principal;

use super::fixtures::{cap, claims, issue, repo, request, LATER, NOW};
use crate::error::Error;
use crate::tokens::{
    attenuate, Capability, Issuer, KeyRing, SigningKey, TokenLifetimes, Verb, Verifier,
};

#[test]
fn tokens_verify_against_the_key_that_signed_them() {
    let old = Issuer::new(SigningKey::generate(1), TokenLifetimes::default());
    let new = Issuer::new(SigningKey::generate(2), TokenLifetimes::default());
    let mut ring = KeyRing::new();
    ring.insert(1, old.public_key(), Some(LATER));
    ring.insert(2, new.public_key(), None);
    let verifier = Verifier::new(ring);
    let alice = Principal::human("alice");
    let from_old = old.issue(&alice, &claims(), None).unwrap();
    let from_new = new.issue(&alice, &claims(), None).unwrap();
    assert!(verifier.authenticate(&from_old, NOW).is_ok());
    assert!(verifier.authenticate(&from_new, NOW).is_ok());

    // A retired key no longer verifies, even for an unexpired token.
    assert!(matches!(
        verifier.authenticate(&from_old, LATER),
        Err(Error::Token(_))
    ));
    // A key the ring does not know is refused.
    let stranger = Issuer::new(SigningKey::generate(3), TokenLifetimes::default());
    let token = stranger.issue(&alice, &claims(), None).unwrap();
    assert!(matches!(
        verifier.authenticate(&token, NOW),
        Err(Error::Token(_))
    ));
    // A key reusing a trusted id with different material fails the signature.
    let impostor = Issuer::new(SigningKey::generate(2), TokenLifetimes::default());
    let token = impostor.issue(&alice, &claims(), None).unwrap();
    assert!(matches!(
        verifier.authenticate(&token, NOW),
        Err(Error::Token(_))
    ));
}

#[test]
fn key_roundtrip() {
    let key = SigningKey::generate(7);
    let again = SigningKey::from_secret_bytes(7, &key.secret_bytes()).unwrap();
    assert_eq!(again.public_key_bytes(), key.public_key_bytes());
    let mut ring = KeyRing::new();
    ring.insert_bytes(7, &key.public_key_bytes(), None).unwrap();
    let issuer = Issuer::new(again, TokenLifetimes::default());
    let token = issue(&issuer, &Principal::human("a"), &cap());
    assert!(Verifier::new(ring)
        .verify(&token, &request("x", Verb::Read))
        .is_ok());
    assert!(SigningKey::from_secret_bytes(1, &[1, 2, 3]).is_err());
}

#[test]
fn every_key_in_a_token_is_p256() {
    use biscuit_auth::format::schema::public_key::Algorithm;
    let issuer = Issuer::generate();
    let token = issue(&issuer, &Principal::agent("bot"), &cap());
    let narrower = Capability::repo_wide(repo(), [Verb::Read], LATER);
    let child = attenuate(&token, &issuer.key_ring(), &narrower).unwrap();
    let parsed = biscuit_auth::Biscuit::from_base64(&child, |_| Ok(issuer.public_key())).unwrap();
    let proto = parsed.container().to_proto();
    let p256 = Algorithm::Secp256r1 as i32;
    assert_eq!(issuer.public_key().to_proto().algorithm, p256);
    assert_eq!(proto.authority.next_key.algorithm, p256);
    assert_eq!(proto.blocks.len(), 1);
    for block in &proto.blocks {
        assert_eq!(block.next_key.algorithm, p256);
    }
}
