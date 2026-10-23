//! SCIM provisioning as Okta and Entra ID drive it: users and groups,
//! teams following groups, and deprovisioning.

use rill_identity::scim::{ListQuery, Scim};
use rill_store::Stores;
use rill_types::identity::{
    AccountStatus, AgentBudgets, AgentRecord, SessionKind, SessionRecord, Sponsor, UserRecord,
};
use rill_types::{Principal, SessionId, Timestamp};
use serde_json::json;

fn scim(stores: &Stores) -> Scim {
    Scim::new(stores.identity(), "acme", "https://rill.test/scim/v2/acme")
}

fn filter(text: &str) -> ListQuery {
    ListQuery {
        filter: Some(text.into()),
        ..ListQuery::default()
    }
}

#[tokio::test]
async fn users_and_groups_become_users_and_teams() {
    let stores = Stores::memory();
    let scim = scim(&stores);
    let ann = scim
        .create_user(&json!({
            "userName": "ann@acme.test", "externalId": "okta-1", "active": true,
            "name": {"givenName": "Ann", "familyName": "Lee"},
            "emails": [{"value": "ann@acme.test", "primary": true}],
        }))
        .await
        .unwrap();
    let ann_id = ann["id"].as_str().unwrap().to_string();
    assert_eq!(ann["displayName"], "Ann Lee");
    let dup = scim
        .create_user(&json!({"userName": "ann@acme.test"}))
        .await
        .unwrap_err();
    assert_eq!(dup.status, 409);
    let found = scim
        .list_users(&filter(r#"externalId eq "okta-1""#))
        .await
        .unwrap();
    assert_eq!(found["totalResults"], 1);
    assert_eq!(found["Resources"][0]["id"], ann_id.as_str());
    assert_eq!(
        scim.list_users(&filter(r#"userName eq "nobody""#))
            .await
            .unwrap()["totalResults"],
        0
    );

    let devs = scim
        .create_group(&json!({"displayName": "devs", "members": [{"value": ann_id}]}))
        .await
        .unwrap();
    let devs_id = devs["id"].as_str().unwrap().to_string();
    let user = stores
        .identity()
        .user("acme", "ann@acme.test")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(user.teams, ["devs"]);
    assert_eq!(
        scim.get_user(&ann_id).await.unwrap()["groups"][0]["display"],
        "devs"
    );

    // Renaming the group renames the team; removing the member drops it.
    scim.patch_group(
        &devs_id,
        &json!({"Operations": [{"op": "replace", "value": {"displayName": "developers"}}]}),
    )
    .await
    .unwrap();
    let user = stores
        .identity()
        .user("acme", "ann@acme.test")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(user.teams, ["developers"]);
    scim.patch_group(
        &devs_id,
        &json!({"Operations": [{"op": "remove", "path": format!("members[value eq \"{ann_id}\"]")}]}),
    )
    .await
    .unwrap();
    let user = stores
        .identity()
        .user("acme", "ann@acme.test")
        .await
        .unwrap()
        .unwrap();
    assert!(user.teams.is_empty());

    scim.patch_group(
        &devs_id,
        &json!({"Operations": [{"op": "add", "path": "members", "value": [{"value": ann_id}]}]}),
    )
    .await
    .unwrap();
    scim.delete_group(&devs_id).await.unwrap();
    let user = stores
        .identity()
        .user("acme", "ann@acme.test")
        .await
        .unwrap()
        .unwrap();
    assert!(user.teams.is_empty(), "deleting a group drops the team");
    assert_eq!(scim.get_group(&devs_id).await.unwrap_err().status, 404);

    let renamed = scim
        .replace_user(&ann_id, &json!({"userName": "eve@acme.test"}))
        .await
        .unwrap_err();
    assert_eq!(renamed.scim_type, Some("mutability"));
}

#[tokio::test]
async fn deprovisioning_ends_sessions_and_suspends_sponsored_agents() {
    let stores = Stores::memory();
    let store = stores.identity();
    let scim = scim(&stores);
    let bob = scim.create_user(&json!({"userName": "bob"})).await.unwrap();
    let bob_id = bob["id"].as_str().unwrap().to_string();
    let now = Timestamp::now();
    let session = SessionRecord {
        id: SessionId::new(),
        org: "acme".into(),
        principal: Principal::human("bob"),
        kind: SessionKind::Device,
        created_at: now,
        expires_at: Timestamp::from_millis(now.millis() + 3_600_000),
        refresh_hash: None,
        revoked_at: None,
        sponsor: None,
        model: None,
    };
    store.create_session(&session).await.unwrap();
    store
        .create_agent(
            &AgentRecord {
                org: "acme".into(),
                principal: Principal::agent("bobs-bot"),
                sponsor: Sponsor::User("bob".into()),
                purpose: "".into(),
                repos: vec!["*".into()],
                paths: vec![],
                models: vec![],
                budgets: AgentBudgets::default(),
                workloads: vec![],
                status: AccountStatus::Active,
                expires_at: None,
                created_by: Principal::human("bob"),
                created_at: now,
                updated_at: now,
                reviewer: None,
            },
            &Principal::human("bob"),
        )
        .await
        .unwrap();

    // Okta deactivates with a PATCH; Entra may send active as a string.
    scim.patch_user(
        &bob_id,
        &json!({"Operations": [{"op": "Replace", "path": "active", "value": "False"}]}),
    )
    .await
    .unwrap();
    assert_eq!(
        store.user("acme", "bob").await.unwrap().unwrap().status,
        AccountStatus::Suspended
    );
    assert!(store
        .session(&session.id)
        .await
        .unwrap()
        .unwrap()
        .revoked_at
        .is_some());
    assert_eq!(
        store
            .agent("acme", "bobs-bot")
            .await
            .unwrap()
            .unwrap()
            .status,
        AccountStatus::Suspended
    );

    // Reactivating the user does not bring the agent back on its own.
    scim.patch_user(
        &bob_id,
        &json!({"Operations": [{"op": "replace", "value": {"active": true}}]}),
    )
    .await
    .unwrap();
    assert!(store
        .user("acme", "bob")
        .await
        .unwrap()
        .unwrap()
        .is_active());
    assert_eq!(
        store
            .agent("acme", "bobs-bot")
            .await
            .unwrap()
            .unwrap()
            .status,
        AccountStatus::Suspended
    );

    scim.delete_user(&bob_id).await.unwrap();
    assert_eq!(
        store.user("acme", "bob").await.unwrap().unwrap().status,
        AccountStatus::Suspended
    );
}

#[tokio::test]
async fn provisioning_adopts_a_user_who_signed_in_first() {
    let stores = Stores::memory();
    let now = Timestamp::now();
    stores
        .identity()
        .create_user(
            &UserRecord {
                org: "acme".into(),
                principal: Principal::human("cy"),
                email: None,
                display_name: None,
                external: vec![],
                status: AccountStatus::Active,
                roles: vec![],
                teams: vec![],
                scim_id: None,
                external_id: None,
                created_at: now,
                updated_at: now,
            },
            &Principal::human("cy"),
        )
        .await
        .unwrap();
    let cy = scim(&stores)
        .create_user(&json!({"userName": "cy", "externalId": "e-9"}))
        .await
        .unwrap();
    let stored = stores.identity().user("acme", "cy").await.unwrap().unwrap();
    assert_eq!(stored.scim_id.as_deref(), cy["id"].as_str());
    assert_eq!(stored.external_id.as_deref(), Some("e-9"));
    assert_eq!(stored.created_at, now, "the same user, adopted");
}
