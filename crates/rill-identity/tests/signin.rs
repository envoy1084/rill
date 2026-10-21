//! Sign-in: ID tokens to users, sessions and their refresh tokens.

use rill_identity::oidc::Issuers;
use rill_identity::signin::{open_session, refresh_session, resolve_user, RefreshToken};
use rill_identity::testing::{MockIdp, MockUser};
use rill_policy::{Issuer, Verifier};
use rill_store::Stores;
use rill_types::identity::{AccountStatus, IdentityProvider, SessionKind};
use rill_types::{Principal, Timestamp};

fn provider(idp: &MockIdp, auto_provision: bool) -> IdentityProvider {
    IdentityProvider {
        org: "acme".into(),
        name: "mock".into(),
        issuer: idp.issuer().into(),
        client_id: "rill".into(),
        client_secret: None,
        cli_client_id: None,
        scopes: vec!["openid".into()],
        username_claim: "preferred_username".into(),
        groups_claim: Some("groups".into()),
        auto_provision,
        created_at: Timestamp::now(),
    }
}

async fn claims(idp: &MockIdp, user: &str) -> rill_identity::oidc::Claims {
    let issuers = Issuers::new(reqwest::Client::new());
    let verifier = issuers
        .verifier(idp.issuer(), vec!["rill".into()])
        .await
        .unwrap();
    verifier
        .verify(&idp.id_token(user, "rill", None), None)
        .await
        .unwrap()
}

#[tokio::test]
async fn sign_in_provisions_once_and_tracks_groups() {
    let idp = MockIdp::start().await;
    idp.add_user(MockUser::new("alice").in_groups(&["core"]));
    let stores = Stores::memory();
    let store = stores.identity();
    let p = provider(&idp, true);
    let now = Timestamp::now();

    let first = resolve_user(&store, &p, &claims(&idp, "alice").await, now)
        .await
        .unwrap();
    assert_eq!(first.principal, Principal::human("alice"));
    assert_eq!(first.teams, ["core"]);
    assert_eq!(first.org, "acme");

    idp.add_user(MockUser::new("alice").in_groups(&["core", "infra"]));
    let again = resolve_user(&store, &p, &claims(&idp, "alice").await, now)
        .await
        .unwrap();
    assert_eq!(again.created_at, first.created_at, "the same user");
    assert_eq!(again.teams, ["core", "infra"]);

    let mut suspended = again.clone();
    suspended.status = AccountStatus::Suspended;
    store
        .update_user(&again, &suspended, &Principal::human("admin"))
        .await
        .unwrap();
    assert!(resolve_user(&store, &p, &claims(&idp, "alice").await, now)
        .await
        .is_err());
}

#[tokio::test]
async fn sign_in_refuses_strangers_and_name_collisions() {
    let idp = MockIdp::start().await;
    idp.add_user(MockUser::new("bob"));
    let stores = Stores::memory();
    let store = stores.identity();
    let now = Timestamp::now();
    assert!(
        resolve_user(
            &store,
            &provider(&idp, false),
            &claims(&idp, "bob").await,
            now
        )
        .await
        .is_err(),
        "an unprovisioned identity is refused without auto-provisioning"
    );

    resolve_user(
        &store,
        &provider(&idp, true),
        &claims(&idp, "bob").await,
        now,
    )
    .await
    .unwrap();
    // Another identity claiming the same name cannot take the account.
    let other = MockUser {
        subject: "someone-else".into(),
        ..MockUser::new("bob")
    };
    idp.add_user(other);
    assert!(resolve_user(
        &store,
        &provider(&idp, true),
        &claims(&idp, "bob").await,
        now
    )
    .await
    .is_err());
}

#[tokio::test]
async fn refresh_tokens_rotate_and_die_with_their_session() {
    let idp = MockIdp::start().await;
    idp.add_user(MockUser::new("carol"));
    let stores = Stores::memory();
    let store = stores.identity();
    let issuer = Issuer::generate();
    let now = Timestamp::now();
    let user = resolve_user(
        &store,
        &provider(&idp, true),
        &claims(&idp, "carol").await,
        now,
    )
    .await
    .unwrap();

    // A session shorter than an access token caps the access token.
    let opened = open_session(&store, &issuer, &user, SessionKind::Device, 60_000, now)
        .await
        .unwrap();
    assert_eq!(opened.expires_at, opened.session.expires_at);
    let verified = Verifier::new(issuer.key_ring())
        .authenticate(&opened.access_token, now)
        .unwrap();
    assert_eq!(verified.claims.session, Some(opened.session.id));
    assert_eq!(verified.principal.name, "carol");

    let text = opened.refresh_token.to_string();
    let parsed: RefreshToken = text.parse().unwrap();
    let refreshed = refresh_session(&store, &issuer, &parsed, now)
        .await
        .unwrap();
    assert_ne!(refreshed.refresh_token.to_string(), text);
    assert!(
        refresh_session(&store, &issuer, &parsed, now)
            .await
            .is_err(),
        "a refresh token works once"
    );

    store
        .revoke_session(&opened.session.id, &Principal::human("admin"))
        .await
        .unwrap();
    assert!(
        refresh_session(&store, &issuer, &refreshed.refresh_token, now)
            .await
            .is_err()
    );
    assert!("garbage".parse::<RefreshToken>().is_err());
}
