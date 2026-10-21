//! The external key services against real servers. Each test runs only
//! when its service is configured, so CI without them skips it:
//!
//! * `RILL_TEST_VAULT_ADDR` (and `VAULT_TOKEN`): a Vault with a Transit key
//!   `rill` at `transit/`.
//! * `RILL_TEST_AWS_KMS_ENDPOINT` (with `--features aws`): LocalStack or
//!   another KMS endpoint; a key is created for the test.

use std::sync::Arc;

use rill_identity::keys::{KeyManager, KeyWrapper, RotationPolicy, VaultTransit};
use rill_policy::{TokenClaims, TokenLifetimes, Verifier};
use rill_store::Stores;
use rill_types::{Principal, Timestamp};

/// A key minted through `wrapper` signs tokens a fresh manager over the
/// same store and service can verify and sign with again.
async fn round_trip(wrapper: Arc<dyn KeyWrapper>) {
    let wrapped = wrapper.wrap(b"secret").await.unwrap();
    assert_ne!(wrapped.ciphertext, b"secret");
    assert_eq!(wrapper.unwrap(&wrapped).await.unwrap(), b"secret");

    let stores = Stores::memory();
    let now = Timestamp::now();
    let keys = KeyManager::new(
        stores.identity(),
        wrapper.clone(),
        RotationPolicy::default(),
        TokenLifetimes::default(),
    );
    keys.ensure_key(now, &Principal::service("rill"))
        .await
        .unwrap();
    let again = KeyManager::new(
        stores.identity(),
        wrapper,
        RotationPolicy::default(),
        TokenLifetimes::default(),
    );
    let set = again.load(now).await.unwrap();
    let claims = TokenClaims::new("default", now, 60_000);
    let token = set
        .issuer
        .unwrap()
        .issue(&Principal::human("alice"), &claims, None)
        .unwrap();
    assert!(Verifier::new(set.ring).authenticate(&token, now).is_ok());
}

#[tokio::test]
async fn vault_transit_wraps_token_keys() {
    let Ok(addr) = std::env::var("RILL_TEST_VAULT_ADDR") else {
        eprintln!("RILL_TEST_VAULT_ADDR not set; skipping");
        return;
    };
    let token = std::env::var("VAULT_TOKEN").unwrap_or_else(|_| "root".into());
    round_trip(Arc::new(VaultTransit::new(
        &addr, &token, "transit", "rill",
    )))
    .await;

    let wrong = VaultTransit::new(&addr, "not-a-token", "transit", "rill");
    assert!(wrong.wrap(b"x").await.is_err());
}

#[cfg(feature = "aws")]
#[tokio::test]
async fn aws_kms_wraps_token_keys() {
    let Ok(endpoint) = std::env::var("RILL_TEST_AWS_KMS_ENDPOINT") else {
        eprintln!("RILL_TEST_AWS_KMS_ENDPOINT not set; skipping");
        return;
    };
    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_config::Region::new("us-east-1"))
        .endpoint_url(&endpoint)
        .load()
        .await;
    let key = aws_sdk_kms::Client::new(&config)
        .create_key()
        .send()
        .await
        .unwrap()
        .key_metadata
        .unwrap()
        .key_id;
    let service =
        rill_identity::keys::AwsKms::connect(&key, Some("us-east-1"), Some(&endpoint)).await;
    round_trip(Arc::new(service)).await;
}
