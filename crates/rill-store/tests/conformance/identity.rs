//! Instance identity state on every backend: keys, sessions, revocations
//! and the change log that announces each.

use rill_store::Stores;
use rill_types::identity::{
    IdentityChange, Revocation, SessionKind, SessionRecord, TokenKeyRecord, WrappedSecret,
};
use rill_types::{Principal, SessionId, Timestamp, TokenId};

fn key(id: u32) -> TokenKeyRecord {
    TokenKeyRecord {
        id,
        public_key: vec![id as u8; 33],
        secret: WrappedSecret {
            backend: "local".into(),
            key_ref: "test".into(),
            ciphertext: vec![1, 2, 3],
        },
        created_at: Timestamp::from_millis(1),
        active_from: Timestamp::from_millis(1),
        not_after: None,
    }
}

pub async fn scenario(name: &str, stores: &Stores) {
    let identity = stores.identity();
    let admin = Principal::human("admin");
    let start = identity.log_count().await.unwrap();
    let cursor = identity.latest_log_entry().await.unwrap();

    identity.add_token_key(&key(1), &admin).await.unwrap();
    assert!(
        identity.add_token_key(&key(1), &admin).await.is_err(),
        "{name}: a key id is taken once"
    );
    identity.add_token_key(&key(2), &admin).await.unwrap();
    let retire = Timestamp::from_millis(500);
    identity.retire_token_key(1, retire, &admin).await.unwrap();
    // Retiring later than already scheduled keeps the earlier date.
    identity
        .retire_token_key(1, Timestamp::from_millis(900), &admin)
        .await
        .unwrap();
    let keys = identity.token_keys().await.unwrap();
    assert_eq!(
        keys.iter().map(|k| k.id).collect::<Vec<_>>(),
        [1, 2],
        "{name}"
    );
    assert_eq!(keys[0].not_after, Some(retire), "{name}");
    assert_eq!(
        identity
            .prune_token_keys(Timestamp::from_millis(501), &admin)
            .await
            .unwrap(),
        1,
        "{name}"
    );

    let session = SessionRecord {
        id: SessionId::new(),
        org: "default".into(),
        principal: Principal::human("alice"),
        kind: SessionKind::Device,
        created_at: Timestamp::now(),
        expires_at: Timestamp::from_millis(Timestamp::now().millis() + 60_000),
        refresh_hash: Some([7; 32]),
        revoked_at: None,
        sponsor: None,
        model: None,
    };
    identity.create_session(&session).await.unwrap();
    assert!(identity.create_session(&session).await.is_err(), "{name}");
    identity.revoke_session(&session.id, &admin).await.unwrap();
    identity.revoke_session(&session.id, &admin).await.unwrap();
    let stored = identity.session(&session.id).await.unwrap().unwrap();
    assert!(!stored.is_live_at(Timestamp::now()), "{name}");
    assert_eq!(
        identity
            .revoked_sessions(Timestamp::now())
            .await
            .unwrap()
            .len(),
        1,
        "{name}"
    );

    let token_id = TokenId::new();
    identity
        .revoke_token(
            &Revocation {
                token_id,
                revoked_by: admin.clone(),
                revoked_at: Timestamp::now(),
                reason: "leaked".into(),
                expires_at: Timestamp::from_millis(Timestamp::now().millis() + 60_000),
            },
            &admin,
        )
        .await
        .unwrap();
    let revoked = identity.revocations(Timestamp::now()).await.unwrap();
    assert_eq!(revoked.len(), 1, "{name}");
    assert_eq!(revoked[0].token_id, token_id, "{name}");

    // Every change was announced, in order, and the counter moved with it.
    let changes: Vec<_> = identity
        .log_after(cursor, 100)
        .await
        .unwrap()
        .into_iter()
        .map(|(_, e)| e.change)
        .collect();
    assert_eq!(
        changes,
        [
            IdentityChange::KeysChanged,
            IdentityChange::KeysChanged,
            IdentityChange::KeysChanged,
            IdentityChange::KeysChanged,
            IdentityChange::SessionRevoked {
                session: session.id
            },
            IdentityChange::TokenRevoked {
                token_id,
                expires_at: revoked[0].expires_at
            },
        ],
        "{name}"
    );
    assert_eq!(identity.log_count().await.unwrap(), start + 6, "{name}");
}
