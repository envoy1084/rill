//! Revocation and key rotation across servers sharing one store.

use rill_identity::keys::KeyServiceConfig;
use rill_proto::rill::v1::IdentityServiceClient;
use rill_runtime::config::IdentityConfig;
use rill_types::identity::{SessionKind, SessionRecord};
use rill_types::{Principal, SessionId, Timestamp};

use super::*;

fn config() -> ServerConfig {
    ServerConfig {
        identity: IdentityConfig {
            keys: KeyServiceConfig::Local {
                master_key_hex: Some("11".repeat(32)),
                master_key_file: None,
            },
            admins: vec!["root".into()],
            ..IdentityConfig::default()
        },
        ..ServerConfig::default()
    }
}

async fn identity(server: &TestServer, token: &str) -> IdentityServiceClient<Svc> {
    IdentityServiceClient::authed(connect(&server.grpc).await, Bearer(Some(token.to_string())))
}

/// `get_caller` with `token` on `server`: the gRPC code it fails with, if any.
async fn who(server: &TestServer, token: &str) -> Option<Code> {
    clients(server, Some(token))
        .await
        .identity
        .get_caller(pb::GetCallerRequest {})
        .await
        .err()
        .map(|s| s.code)
}

async fn refused_within(server: &TestServer, token: &str, limit: Duration) -> Duration {
    let started = tokio::time::Instant::now();
    while who(server, token).await.is_none() {
        assert!(started.elapsed() < limit, "still accepted after {limit:?}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    started.elapsed()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_revoked_token_is_refused_by_every_server() {
    let stores = rill_store::Stores::memory();
    let a = TestServer::start_on(config(), stores.clone()).await;
    let b = TestServer::start_on(config(), stores.clone()).await;
    let alice = a.token_for(&Principal::human("alice"));
    let root = a.token_for(&Principal::human("root"));
    assert_eq!(
        who(&b, &alice).await,
        None,
        "a token signed by one server works on another"
    );

    // The holder revokes it on one server; the other refuses it within
    // seconds, through the identity log.
    identity(&a, &alice)
        .await
        .revoke_token(pb::RevokeTokenRequest {
            request_id: String::new(),
            target: Some(pb::revoke_token_request::Target::Token(alice.clone())),
            reason: "logout".into(),
        })
        .await
        .unwrap();
    assert_eq!(who(&a, &alice).await, Some(Code::Unauthenticated));
    let took = refused_within(&b, &alice, Duration::from_secs(10)).await;
    assert!(took < Duration::from_secs(10));

    // Only administrators revoke by id.
    let bob = a.token_for(&Principal::human("bob"));
    let bob_id = clients(&a, Some(&bob))
        .await
        .identity
        .get_caller(pb::GetCallerRequest {})
        .await
        .unwrap()
        .into_owned()
        .token_id
        .unwrap();
    let by_id = |id: String| pb::RevokeTokenRequest {
        request_id: String::new(),
        target: Some(pb::revoke_token_request::Target::TokenId(id)),
        reason: "compromised".into(),
    };
    let mallory = a.token_for(&Principal::human("mallory"));
    let denied = identity(&a, &mallory)
        .await
        .revoke_token(by_id(bob_id.clone()))
        .await
        .unwrap_err();
    assert_eq!(denied.code, Code::PermissionDenied);
    identity(&b, &root)
        .await
        .revoke_token(by_id(bob_id))
        .await
        .unwrap();
    refused_within(&a, &bob, Duration::from_secs(10)).await;
    assert_eq!(who(&a, &mallory).await, None, "other tokens are untouched");

    a.stop().await;
    b.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn revoking_a_session_refuses_every_token_issued_under_it() {
    let stores = rill_store::Stores::memory();
    let a = TestServer::start_on(config(), stores.clone()).await;
    let b = TestServer::start_on(config(), stores.clone()).await;
    let alice = Principal::human("alice");
    let session = SessionRecord {
        id: SessionId::new(),
        org: "default".into(),
        principal: alice.clone(),
        kind: SessionKind::Device,
        created_at: Timestamp::now(),
        expires_at: Timestamp::from_millis(Timestamp::now().millis() + 3_600_000),
        refresh_hash: None,
        revoked_at: None,
        sponsor: None,
        model: None,
    };
    stores.identity().create_session(&session).await.unwrap();
    let issuer = a.state.identity.issuer().unwrap();
    let mint = || {
        let claims = rill_policy::TokenClaims::new("default", Timestamp::now(), 600_000)
            .with_session(session.id);
        issuer.issue(&alice, &claims, None).unwrap()
    };
    let (first, second) = (mint(), mint());
    let other = a.token_for(&alice);

    identity(&a, &first)
        .await
        .revoke_session(pb::RevokeSessionRequest {
            request_id: String::new(),
            session: session.id.to_string(),
        })
        .await
        .unwrap();
    refused_within(&b, &first, Duration::from_secs(10)).await;
    refused_within(&b, &second, Duration::from_secs(10)).await;
    assert_eq!(
        who(&b, &other).await,
        None,
        "tokens outside the session still work"
    );

    a.stop().await;
    b.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn tokens_from_a_rotated_key_verify_on_every_server() {
    let stores = rill_store::Stores::memory();
    let mut fast = config();
    fast.identity.rotation.activation_delay_secs = 1;
    let a = TestServer::start_on(fast.clone(), stores.clone()).await;
    let b = TestServer::start_on(fast, stores.clone()).await;
    let root = a.token_for(&Principal::human("root"));
    let before = a.token_for(&Principal::human("alice"));

    let rotated = identity(&a, &root)
        .await
        .rotate_token_keys(pb::RotateTokenKeysRequest {
            request_id: String::new(),
        })
        .await
        .unwrap()
        .into_owned();
    let new_key = rotated.key_id.unwrap();
    // Once the new key activates, servers sign with it and every server
    // verifies both old and new tokens.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        a.state.identity.reload_keys().await.unwrap();
        if a.state.identity.issuer().unwrap().key_id() == new_key {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "key never activated"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let after = a.token_for(&Principal::human("alice"));
    assert_eq!(who(&b, &after).await, None);
    assert_eq!(who(&b, &before).await, None);

    let listed = identity(&b, &root)
        .await
        .list_token_keys(pb::ListTokenKeysRequest {})
        .await
        .unwrap()
        .into_owned()
        .keys;
    assert_eq!(listed.len(), 2);
    assert!(listed[0].not_after.is_some(), "the old key retires");
    assert_eq!(listed[1].backend, "local");
    a.stop().await;
    b.stop().await;
}
