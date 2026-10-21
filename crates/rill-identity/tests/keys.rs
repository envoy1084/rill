//! Token keys over a shared store: created once, rotated ahead of use,
//! retired after the tokens they signed have expired.

use std::sync::Arc;

use rill_identity::keys::{KeyManager, KeyWrapper, LocalKeyService, RotationPolicy};
use rill_policy::{TokenClaims, TokenLifetimes, Verifier};
use rill_store::Stores;
use rill_types::{Principal, Timestamp};

const T0: i64 = 1_800_000_000_000;

fn at(ms: i64) -> Timestamp {
    Timestamp::from_millis(T0 + ms)
}

fn manager(stores: &Stores, master: u8) -> KeyManager {
    let wrapper: Arc<dyn KeyWrapper> = Arc::new(LocalKeyService::new(&[master; 32]).unwrap());
    KeyManager::new(
        stores.identity(),
        wrapper,
        RotationPolicy::default(),
        TokenLifetimes::default(),
    )
}

fn admin() -> Principal {
    Principal::service("rill")
}

fn token(set: &rill_identity::keys::KeySet, now: Timestamp) -> String {
    let claims = TokenClaims::new("default", now, 3_600_000);
    set.issuer
        .as_ref()
        .expect("a signing key is active")
        .issue(&Principal::human("alice"), &claims, None)
        .unwrap()
}

#[tokio::test]
async fn a_fresh_instance_gets_one_key_even_when_servers_race() {
    let stores = Stores::memory();
    let (a, b) = (manager(&stores, 1), manager(&stores, 1));
    let actor = admin();
    let (ra, rb) = tokio::join!(a.ensure_key(at(0), &actor), b.ensure_key(at(0), &actor));
    ra.unwrap();
    rb.unwrap();
    assert_eq!(stores.identity().token_keys().await.unwrap().len(), 1);

    // Both servers sign with it, and each verifies the other's tokens.
    let (sa, sb) = (a.load(at(0)).await.unwrap(), b.load(at(0)).await.unwrap());
    let t = token(&sa, at(0));
    assert!(Verifier::new(sb.ring.clone())
        .authenticate(&t, at(1))
        .is_ok());
}

#[tokio::test]
async fn rotation_activates_later_and_old_tokens_verify_until_they_expire() {
    let stores = Stores::memory();
    let keys = manager(&stores, 1);
    let policy = *keys.policy();
    keys.ensure_key(at(0), &admin()).await.unwrap();
    let first = keys.load(at(0)).await.unwrap();
    let old_token = token(&first, at(0));

    let rotated = keys.rotate(at(1_000), &admin()).await.unwrap().unwrap();
    // Until the new key activates, servers keep signing with the old one
    // but already trust the new one.
    let before = keys.load(at(1_000)).await.unwrap();
    assert_eq!(before.issuer.as_ref().unwrap().key_id(), 1);
    assert!(before.ring.contains(rotated));
    let activates = 1_000 + policy.activation_delay_ms;
    assert_eq!(before.next_activation, Some(at(activates)));

    let after = keys.load(at(activates)).await.unwrap();
    assert_eq!(after.issuer.as_ref().unwrap().key_id(), rotated);
    let verifier = Verifier::new(after.ring.clone());
    assert!(verifier.authenticate(&old_token, at(activates)).is_ok());
    assert!(verifier
        .authenticate(&token(&after, at(activates)), at(activates))
        .is_ok());

    // The old key retires once nothing it signed can still be valid.
    let retire = activates + TokenLifetimes::default().service_ms + policy.grace_ms;
    let late = keys.load(at(retire)).await.unwrap();
    assert!(Verifier::new(late.ring.clone())
        .authenticate(&old_token, at(retire))
        .is_err());
}

#[tokio::test]
async fn maintenance_rotates_when_due_and_prunes_retired_keys() {
    let stores = Stores::memory();
    let keys = manager(&stores, 1);
    let policy = *keys.policy();
    keys.ensure_key(at(0), &admin()).await.unwrap();
    assert!(!keys.maintain(at(1_000), &admin()).await.unwrap());
    let due = policy.rotate_after_ms;
    assert!(keys.maintain(at(due), &admin()).await.unwrap());
    assert_eq!(stores.identity().token_keys().await.unwrap().len(), 2);

    let gone = due
        + policy.activation_delay_ms
        + TokenLifetimes::default().service_ms
        + policy.grace_ms
        + policy.keep_retired_ms
        + 1;
    keys.maintain(at(gone), &admin()).await.unwrap();
    let left: Vec<_> = stores
        .identity()
        .token_keys()
        .await
        .unwrap()
        .iter()
        .map(|k| k.id)
        .collect();
    assert_eq!(left, [2], "key 1 pruned once retired long enough");
}

#[tokio::test]
async fn a_changed_key_service_mints_a_key_it_can_open() {
    let stores = Stores::memory();
    manager(&stores, 1)
        .ensure_key(at(0), &admin())
        .await
        .unwrap();
    let moved = manager(&stores, 2);
    // The stored key opens only under the old master key.
    assert!(moved.load(at(0)).await.unwrap().issuer.is_none());
    moved.ensure_key(at(0), &admin()).await.unwrap();
    let set = moved.load(at(0)).await.unwrap();
    assert_eq!(set.issuer.as_ref().unwrap().key_id(), 2);
    assert!(
        set.ring.contains(1),
        "the old key still verifies its tokens"
    );
}
