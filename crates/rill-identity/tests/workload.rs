//! Exchanging a workload's JWT for an agent: binding, audience, issuer
//! trust, registration, model and sponsor checks.

use std::collections::BTreeMap;

use rill_identity::agents::{exchange_workload, WorkloadKeys};
use rill_identity::oidc::Issuers;
use rill_identity::testing::{MockIdp, MockUser};
use rill_store::Stores;
use rill_types::identity::{
    AccountStatus, AgentBudgets, AgentRecord, ExternalIdentity, IssuerKeys, OrgIdentitySettings,
    Sponsor, UserRecord, WorkloadBinding, WorkloadIssuer,
};
use rill_types::{Principal, Timestamp};
use serde_json::json;

fn admin() -> Principal {
    Principal::human("admin")
}

fn agent(name: &str, issuer: &str, subject: &str) -> AgentRecord {
    AgentRecord {
        org: "acme".into(),
        principal: Principal::agent(name),
        sponsor: Sponsor::User("ann".into()),
        purpose: "ci".into(),
        repos: vec!["*".into()],
        paths: vec![],
        models: vec![],
        budgets: AgentBudgets::default(),
        workloads: vec![WorkloadBinding {
            issuer: issuer.into(),
            subject: subject.into(),
            claims: BTreeMap::from([("namespace".into(), "ci".into())]),
        }],
        status: AccountStatus::Active,
        expires_at: None,
        created_by: admin(),
        created_at: Timestamp::now(),
        updated_at: Timestamp::now(),
    }
}

async fn setup(keys: IssuerKeys) -> (MockIdp, Stores, WorkloadKeys) {
    let idp = MockIdp::start().await;
    idp.add_user(MockUser::new("ann"));
    let stores = Stores::memory();
    let store = stores.identity();
    store
        .create_user(
            &UserRecord {
                org: "acme".into(),
                principal: Principal::human("ann"),
                email: None,
                display_name: None,
                external: vec![ExternalIdentity {
                    issuer: idp.issuer().into(),
                    subject: "sub-ann".into(),
                }],
                status: AccountStatus::Active,
                roles: vec![],
                teams: vec![],
                scim_id: None,
                created_at: Timestamp::now(),
                updated_at: Timestamp::now(),
            },
            &admin(),
        )
        .await
        .unwrap();
    store
        .put_workload_issuer(
            &WorkloadIssuer {
                org: "acme".into(),
                name: "cluster".into(),
                issuer: idp.issuer().into(),
                keys,
                audiences: vec!["rill".into()],
                created_at: Timestamp::now(),
            },
            &admin(),
        )
        .await
        .unwrap();
    store
        .create_agent(
            &agent("builder", idp.issuer(), "system:serviceaccount:ci:*"),
            &admin(),
        )
        .await
        .unwrap();
    let keys = WorkloadKeys::new(Issuers::new(reqwest::Client::new()));
    (idp, stores, keys)
}

fn workload_token(idp: &MockIdp, subject: &str, audience: &str, namespace: &str) -> String {
    let now = jsonwebtoken::get_current_timestamp();
    idp.jwt(&json!({
        "iss": idp.issuer(), "sub": subject, "aud": audience,
        "exp": now + 600, "iat": now, "namespace": namespace,
    }))
}

#[tokio::test]
async fn a_bound_workload_becomes_its_agent() {
    let (idp, stores, keys) = setup(IssuerKeys::Discovery).await;
    let store = stores.identity();
    let now = Timestamp::now();
    let token = workload_token(&idp, "system:serviceaccount:ci:runner", "rill", "ci");
    let (agent, _) = exchange_workload(&store, &keys, "acme", &token, None, None, now)
        .await
        .unwrap();
    assert_eq!(agent.principal, Principal::agent("builder"));

    let refuse = |token: String, org: &'static str| {
        let (store, keys) = (store.clone(), keys.clone());
        async move {
            exchange_workload(&store, &keys, org, &token, None, None, now)
                .await
                .is_err()
        }
    };
    assert!(
        refuse(
            workload_token(&idp, "system:serviceaccount:ci:runner", "other", "ci"),
            "acme"
        )
        .await,
        "audience"
    );
    assert!(
        refuse(
            workload_token(&idp, "system:serviceaccount:prod:x", "rill", "ci"),
            "acme"
        )
        .await,
        "subject"
    );
    assert!(
        refuse(
            workload_token(&idp, "system:serviceaccount:ci:runner", "rill", "prod"),
            "acme"
        )
        .await,
        "claim"
    );
    assert!(
        refuse(token.clone(), "other-org").await,
        "an issuer is trusted per org"
    );
}

#[tokio::test]
async fn registration_model_and_sponsor_gate_the_exchange() {
    let (idp, stores, keys) = setup(IssuerKeys::Discovery).await;
    let store = stores.identity();
    let now = Timestamp::now();
    let token = workload_token(&idp, "system:serviceaccount:ci:runner", "rill", "ci");
    let exchange = |model: Option<&'static str>| {
        let (store, keys, token) = (store.clone(), keys.clone(), token.clone());
        async move { exchange_workload(&store, &keys, "acme", &token, None, model, now).await }
    };

    let before = store.agent("acme", "builder").await.unwrap().unwrap();
    let limited = AgentRecord {
        models: vec!["model-a".into()],
        ..before.clone()
    };
    store
        .update_agent(&before, &limited, &admin())
        .await
        .unwrap();
    assert!(exchange(Some("model-a")).await.is_ok());
    assert!(
        exchange(Some("model-b")).await.is_err(),
        "model not allowed"
    );
    assert!(exchange(None).await.is_err(), "an allow-list needs a model");

    store
        .put_org_settings(
            &OrgIdentitySettings {
                org: "acme".into(),
                blocked_models: vec!["model-a".into()],
            },
            &admin(),
        )
        .await
        .unwrap();
    assert!(exchange(Some("model-a")).await.is_err(), "blocked model");
    store
        .put_org_settings(
            &OrgIdentitySettings {
                org: "acme".into(),
                blocked_models: vec![],
            },
            &admin(),
        )
        .await
        .unwrap();

    let ann = store.user("acme", "ann").await.unwrap().unwrap();
    let gone = UserRecord {
        status: AccountStatus::Suspended,
        ..ann.clone()
    };
    store.update_user(&ann, &gone, &admin()).await.unwrap();
    assert!(
        exchange(Some("model-a")).await.is_err(),
        "sponsor suspended"
    );
    store.update_user(&gone, &ann, &admin()).await.unwrap();

    let suspended = AgentRecord {
        status: AccountStatus::Suspended,
        ..limited.clone()
    };
    store
        .update_agent(&limited, &suspended, &admin())
        .await
        .unwrap();
    assert!(exchange(Some("model-a")).await.is_err(), "agent suspended");
}

#[tokio::test]
async fn inline_keys_and_ambiguous_bindings() {
    let idp = MockIdp::start().await;
    let jwks = reqwest::get(format!("{}/jwks", idp.issuer()))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let (other_idp, stores, keys) = setup(IssuerKeys::Inline(jwks)).await;
    drop(other_idp);
    let store = stores.identity();
    // `setup` trusted its own provider; trust this one's inline keys instead.
    let mut trusted = store.workload_issuers("acme").await.unwrap().remove(0);
    trusted.issuer = idp.issuer().into();
    store.put_workload_issuer(&trusted, &admin()).await.unwrap();
    for name in ["one", "two"] {
        store
            .create_agent(&agent(name, idp.issuer(), "spiffe://acme/ci/*"), &admin())
            .await
            .unwrap();
    }
    let now = Timestamp::now();
    let token = workload_token(&idp, "spiffe://acme/ci/runner", "rill", "ci");
    assert!(
        exchange_workload(&store, &keys, "acme", &token, None, None, now)
            .await
            .is_err(),
        "two bindings match"
    );
    let (agent, _) = exchange_workload(&store, &keys, "acme", &token, Some("two"), None, now)
        .await
        .unwrap();
    assert_eq!(agent.principal.name, "two");
}
