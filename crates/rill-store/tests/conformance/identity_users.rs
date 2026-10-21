//! Users, identity providers and sign-ins on every backend.

use rill_store::Stores;
use rill_types::identity::{
    AccountStatus, AgentBudgets, AgentRecord, ExternalIdentity, IdentityProvider, IssuerKeys,
    OrgIdentitySettings, PendingLogin, SessionKind, SessionRecord, Sponsor, UserRecord,
    WorkloadIssuer,
};
use rill_types::{Principal, SessionId, Timestamp};

fn user(org: &str, name: &str, subject: &str) -> UserRecord {
    UserRecord {
        org: org.into(),
        principal: Principal::human(name),
        email: Some(format!("{name}@example.com")),
        display_name: None,
        external: vec![ExternalIdentity {
            issuer: "https://idp".into(),
            subject: subject.into(),
        }],
        status: AccountStatus::Active,
        roles: vec![],
        teams: vec![],
        scim_id: None,
        external_id: None,
        created_at: Timestamp::from_millis(1),
        updated_at: Timestamp::from_millis(1),
    }
}

fn provider(org: &str, name: &str, issuer: &str) -> IdentityProvider {
    IdentityProvider {
        org: org.into(),
        name: name.into(),
        issuer: issuer.into(),
        client_id: "rill".into(),
        client_secret: None,
        cli_client_id: None,
        scopes: vec!["openid".into()],
        username_claim: "preferred_username".into(),
        groups_claim: None,
        auto_provision: true,
        created_at: Timestamp::from_millis(1),
    }
}

pub async fn scenario(name: &str, stores: &Stores) {
    let identity = stores.identity();
    let admin = Principal::human("admin");

    let alice = user("acme", "alice", "s-1");
    identity.create_user(&alice, &admin).await.unwrap();
    assert!(
        identity
            .create_user(&user("other", "alice", "s-9"), &admin)
            .await
            .is_err(),
        "{name}: a principal name is unique across orgs"
    );
    assert!(
        identity
            .create_user(&user("acme", "bob", "s-1"), &admin)
            .await
            .is_err(),
        "{name}: an external identity signs in as one user"
    );
    assert_eq!(
        identity
            .principal_org(&alice.principal)
            .await
            .unwrap()
            .as_deref(),
        Some("acme"),
        "{name}"
    );
    let by_subject = identity
        .user_by_external(&alice.external[0])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(by_subject, alice, "{name}");

    let mut moved = alice.clone();
    moved.external = vec![ExternalIdentity {
        issuer: "https://idp".into(),
        subject: "s-2".into(),
    }];
    moved.status = AccountStatus::Suspended;
    identity.update_user(&alice, &moved, &admin).await.unwrap();
    assert!(
        identity.update_user(&alice, &moved, &admin).await.is_err(),
        "{name}: an update from a stale read conflicts"
    );
    assert!(
        identity
            .user_by_external(&alice.external[0])
            .await
            .unwrap()
            .is_none(),
        "{name}"
    );
    assert_eq!(
        identity
            .user_by_external(&moved.external[0])
            .await
            .unwrap()
            .unwrap()
            .status,
        AccountStatus::Suspended,
        "{name}"
    );
    assert_eq!(identity.users("acme").await.unwrap().len(), 1, "{name}");

    identity
        .put_provider(&provider("acme", "okta", "https://okta"), &admin)
        .await
        .unwrap();
    assert!(
        identity
            .put_provider(&provider("other", "okta", "https://okta"), &admin)
            .await
            .is_err(),
        "{name}: an issuer belongs to one provider"
    );
    identity
        .put_provider(&provider("acme", "okta", "https://okta2"), &admin)
        .await
        .unwrap();
    assert!(
        identity
            .provider_by_issuer("https://okta")
            .await
            .unwrap()
            .is_none(),
        "{name}"
    );
    assert_eq!(
        identity
            .provider_by_issuer("https://okta2")
            .await
            .unwrap()
            .unwrap()
            .name,
        "okta",
        "{name}"
    );
    assert!(
        identity
            .delete_provider("acme", "okta", &admin)
            .await
            .unwrap(),
        "{name}"
    );
    assert!(
        identity
            .provider_by_issuer("https://okta2")
            .await
            .unwrap()
            .is_none(),
        "{name}"
    );

    let now = Timestamp::now();
    let login = PendingLogin {
        state: "st".into(),
        org: "acme".into(),
        provider: "okta".into(),
        nonce: "n".into(),
        pkce_verifier: "v".into(),
        return_to: None,
        expires_at: Timestamp::from_millis(now.millis() + 60_000),
    };
    identity.put_pending_login(&login).await.unwrap();
    assert_eq!(
        identity.take_pending_login("st", now).await.unwrap(),
        Some(login),
        "{name}"
    );
    assert_eq!(
        identity.take_pending_login("st", now).await.unwrap(),
        None,
        "{name}: a sign-in completes once"
    );

    let session = SessionRecord {
        id: SessionId::new(),
        org: "acme".into(),
        principal: Principal::human("alice"),
        kind: SessionKind::Browser,
        created_at: now,
        expires_at: Timestamp::from_millis(now.millis() + 60_000),
        refresh_hash: Some([1; 32]),
        revoked_at: None,
        sponsor: None,
        model: None,
    };
    identity.create_session(&session).await.unwrap();
    assert!(
        identity
            .rotate_refresh(&session.id, [1; 32], [2; 32])
            .await
            .unwrap()
            .is_some(),
        "{name}"
    );
    assert!(
        identity
            .rotate_refresh(&session.id, [1; 32], [3; 32])
            .await
            .unwrap()
            .is_none(),
        "{name}: a refresh secret works once"
    );

    agents_and_workloads(name, stores).await;
}

async fn agents_and_workloads(name: &str, stores: &Stores) {
    let identity = stores.identity();
    let admin = Principal::human("admin");
    let agent = AgentRecord {
        org: "acme".into(),
        principal: Principal::agent("alice"),
        sponsor: Sponsor::User("alice".into()),
        purpose: "tests".into(),
        repos: vec!["*".into()],
        paths: vec![],
        models: vec![],
        budgets: AgentBudgets::default(),
        workloads: vec![],
        status: AccountStatus::Active,
        expires_at: None,
        created_by: admin.clone(),
        created_at: Timestamp::from_millis(1),
        updated_at: Timestamp::from_millis(1),
    };
    // A user and an agent may share a name; two agents may not.
    identity.create_agent(&agent, &admin).await.unwrap();
    let elsewhere = AgentRecord {
        org: "other".into(),
        ..agent.clone()
    };
    assert!(
        identity.create_agent(&elsewhere, &admin).await.is_err(),
        "{name}"
    );
    let suspended = AgentRecord {
        status: AccountStatus::Suspended,
        ..agent.clone()
    };
    identity
        .update_agent(&agent, &suspended, &admin)
        .await
        .unwrap();
    assert!(
        identity
            .update_agent(&agent, &suspended, &admin)
            .await
            .is_err(),
        "{name}"
    );
    assert_eq!(
        identity.agents("acme").await.unwrap(),
        [suspended],
        "{name}"
    );

    let issuer = WorkloadIssuer {
        org: "acme".into(),
        name: "k8s".into(),
        issuer: "https://kubernetes.default.svc".into(),
        keys: IssuerKeys::Inline("{\"keys\":[]}".into()),
        audiences: vec!["rill".into()],
        created_at: Timestamp::from_millis(1),
    };
    identity.put_workload_issuer(&issuer, &admin).await.unwrap();
    assert_eq!(
        identity.workload_issuers("acme").await.unwrap(),
        [issuer],
        "{name}"
    );
    assert!(
        identity
            .delete_workload_issuer("acme", "k8s", &admin)
            .await
            .unwrap(),
        "{name}"
    );
    assert!(
        identity.workload_issuers("acme").await.unwrap().is_empty(),
        "{name}"
    );

    assert!(
        identity
            .org_settings("acme")
            .await
            .unwrap()
            .blocked_models
            .is_empty(),
        "{name}"
    );
    let settings = OrgIdentitySettings {
        org: "acme".into(),
        blocked_models: vec!["model-x".into()],
    };
    identity.put_org_settings(&settings, &admin).await.unwrap();
    assert_eq!(
        identity.org_settings("acme").await.unwrap(),
        settings,
        "{name}"
    );
}
