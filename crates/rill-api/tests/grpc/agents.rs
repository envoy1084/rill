//! Agents: registration, the workload exchange, registered scope, and
//! suspension of the agent or its sponsor.

use rill_api::config::IdentityConfig;
use rill_identity::keys::KeyServiceConfig;
use rill_identity::testing::MockIdp;
use rill_proto::rill::v1::identity_service_client::IdentityServiceClient;
use rill_proto::rill::v1::session_service_client::SessionServiceClient;
use rill_types::identity::{AccountStatus, OrgIdentitySettings, UserRecord};
use rill_types::{Principal, Timestamp};

use super::*;

fn config() -> ServerConfig {
    ServerConfig {
        identity: IdentityConfig {
            keys: KeyServiceConfig::Local {
                master_key_hex: Some("44".repeat(32)),
                master_key_file: None,
            },
            admins: vec!["root".into()],
            ..IdentityConfig::default()
        },
        ..ServerConfig::default()
    }
}

async fn identity(server: &TestServer, token: &str) -> IdentityServiceClient<Svc> {
    IdentityServiceClient::with_interceptor(connect(&server.grpc).await, Bearer(Some(token.into())))
}

fn user(name: &str) -> UserRecord {
    UserRecord {
        org: "default".into(),
        principal: Principal::human(name),
        email: None,
        display_name: None,
        external: vec![],
        status: AccountStatus::Active,
        roles: vec![],
        teams: vec![],
        scim_id: None,
        created_at: Timestamp::now(),
        updated_at: Timestamp::now(),
    }
}

fn workload_jwt(idp: &MockIdp, subject: &str) -> String {
    let now = (Timestamp::now().millis() / 1000) as u64;
    idp.jwt(&serde_json::json!({
        "iss": idp.issuer(), "sub": subject, "aud": "rill", "exp": now + 600, "iat": now,
    }))
}

async fn exchange(
    server: &TestServer,
    jwt: &str,
    model: Option<&str>,
) -> Result<pb::SessionGrant, Status> {
    SessionServiceClient::new(connect(&server.grpc).await)
        .exchange_token(pb::ExchangeTokenRequest {
            subject_token: jwt.into(),
            subject_token_type: "urn:ietf:params:oauth:token-type:jwt".into(),
            org: "default".into(),
            agent: None,
            model: model.map(String::from),
        })
        .await
        .map(|r| r.into_inner())
}

async fn refused_within(server: &TestServer, token: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let mut c = clients(server, Some(token)).await;
        match c.admin.who_am_i(pb::WhoAmIRequest {}).await {
            Err(s) if s.code() == Code::Unauthenticated => return,
            _ => {}
        }
        assert!(tokio::time::Instant::now() < deadline, "still accepted");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn spec(name: &str, sponsor: &str, issuer: &str) -> pb::AgentSpec {
    pb::AgentSpec {
        org: "default".into(),
        name: name.into(),
        sponsor: sponsor.into(),
        purpose: "tests".into(),
        repos: vec!["allowed".into()],
        paths: vec!["src".into()],
        models: vec![],
        budgets: None,
        workloads: vec![pb::WorkloadBinding {
            issuer: issuer.into(),
            subject: format!("ci:{name}"),
            claims: Default::default(),
        }],
        expires_at: None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_workload_acts_as_its_agent_within_the_registered_scope() {
    let idp = MockIdp::start().await;
    let stores = rill_store::Stores::memory();
    stores.ensure_default_org().await.unwrap();
    stores
        .identity()
        .create_user(&user("ann"), &Principal::human("root"))
        .await
        .unwrap();
    let a = TestServer::start_on(config(), stores.clone()).await;
    let b = TestServer::start_on(config(), stores.clone()).await;
    let root = a.token_for(&Principal::human("root"));
    let mut admin = identity(&a, &root).await;
    admin
        .put_workload_issuer(pb::PutWorkloadIssuerRequest {
            operation_id: Bytes::new(),
            issuer: Some(pb::WorkloadIssuerSpec {
                org: "default".into(),
                name: "ci".into(),
                issuer: idp.issuer().into(),
                keys: Some(pb::workload_issuer_spec::Keys::Discovery(true)),
                audiences: vec!["rill".into()],
            }),
        })
        .await
        .unwrap();
    admin
        .register_agent(pb::RegisterAgentRequest {
            operation_id: Bytes::new(),
            agent: Some(spec("builder", "user:ann", idp.issuer())),
        })
        .await
        .unwrap();

    let grant = exchange(&a, &workload_jwt(&idp, "ci:builder"), Some("model-a"))
        .await
        .unwrap();
    assert!(
        grant.refresh_token.is_none(),
        "agents exchange again instead"
    );
    let token = grant.access_token;
    let me = clients(&b, Some(&token))
        .await
        .admin
        .who_am_i(pb::WhoAmIRequest {})
        .await
        .unwrap()
        .into_inner();
    assert_eq!(me.principal.unwrap().name, "builder");
    assert_eq!(me.sponsor.as_deref(), Some("user:ann"));
    assert_eq!(me.model.as_deref(), Some("model-a"));
    assert!(exchange(&a, &workload_jwt(&idp, "ci:stranger"), None)
        .await
        .is_err());

    // Relations would let the agent write anywhere; its registration does not.
    let mut root_clients = clients(&a, Some(&root)).await;
    for repo in ["allowed", "other"] {
        root_clients
            .repo
            .create_repo(pb::CreateRepoRequest {
                operation_id: Bytes::new(),
                repo: repo.into(),
                description: None,
                windows_safe: false,
            })
            .await
            .unwrap();
        root_clients
            .admin
            .add_relation(pb::AddRelationRequest {
                operation_id: Bytes::new(),
                relation: Some(pb::Relation {
                    object: format!("repo:{repo}"),
                    relation: "writer".into(),
                    subject: format!("principal:{}", Principal::agent("builder").id),
                }),
            })
            .await
            .unwrap();
    }
    let mut agent = clients(&b, Some(&token)).await;
    let write = |repo: &str, path: &str| pb::ApplyEditsRequest {
        operation_id: Bytes::new(),
        repo: repo.into(),
        base: None,
        edits: vec![set_edit(path, b"x\n")],
        message: "m".into(),
        intent: None,
        provenance: None,
        change_id: None,
    };
    agent
        .repo
        .apply_edits(write("allowed", "src/a.rs"))
        .await
        .unwrap();
    let outside = agent
        .repo
        .apply_edits(write("allowed", "docs/a.md"))
        .await
        .unwrap_err();
    assert_eq!(outside.code(), Code::PermissionDenied);
    let other = agent
        .repo
        .apply_edits(write("other", "src/a.rs"))
        .await
        .unwrap_err();
    assert_eq!(other.code(), Code::PermissionDenied);

    // Suspending the sponsor stops the agent on every server.
    let ann = stores
        .identity()
        .user("default", "ann")
        .await
        .unwrap()
        .unwrap();
    let gone = UserRecord {
        status: AccountStatus::Suspended,
        ..ann.clone()
    };
    stores
        .identity()
        .update_user(&ann, &gone, &Principal::human("root"))
        .await
        .unwrap();
    refused_within(&b, &token).await;
    assert!(exchange(&a, &workload_jwt(&idp, "ci:builder"), None)
        .await
        .is_err());
    stores
        .identity()
        .update_user(&gone, &ann, &Principal::human("root"))
        .await
        .unwrap();

    // So do suspending the agent, and blocking the model it runs as.
    let token = exchange(&a, &workload_jwt(&idp, "ci:builder"), Some("model-a"))
        .await
        .unwrap()
        .access_token;
    stores
        .identity()
        .put_org_settings(
            &OrgIdentitySettings {
                org: "default".into(),
                blocked_models: vec!["model-a".into()],
            },
            &Principal::human("root"),
        )
        .await
        .unwrap();
    refused_within(&b, &token).await;
    let token = exchange(&a, &workload_jwt(&idp, "ci:builder"), Some("model-b"))
        .await
        .unwrap()
        .access_token;
    admin
        .update_agent(pb::UpdateAgentRequest {
            operation_id: Bytes::new(),
            org: "default".into(),
            name: "builder".into(),
            suspended: Some(true),
            spec: None,
        })
        .await
        .unwrap();
    refused_within(&b, &token).await;
    a.stop().await;
    b.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn users_register_only_agents_they_sponsor() {
    let stores = rill_store::Stores::memory();
    stores.ensure_default_org().await.unwrap();
    for name in ["ann", "bob"] {
        stores
            .identity()
            .create_user(&user(name), &Principal::human("root"))
            .await
            .unwrap();
    }
    let server = TestServer::start_on(config(), stores.clone()).await;
    let ann = server.token_for(&Principal::human("ann"));
    let mut svc = identity(&server, &ann).await;
    svc.register_agent(pb::RegisterAgentRequest {
        operation_id: Bytes::new(),
        agent: Some(spec("anns-bot", "user:ann", "https://ci")),
    })
    .await
    .unwrap();
    let denied = svc
        .register_agent(pb::RegisterAgentRequest {
            operation_id: Bytes::new(),
            agent: Some(spec("bobs-bot", "user:bob", "https://ci")),
        })
        .await
        .unwrap_err();
    assert_eq!(denied.code(), Code::PermissionDenied);
    let listed = svc
        .list_agents(pb::ListAgentsRequest {
            org: "default".into(),
        })
        .await
        .unwrap()
        .into_inner()
        .agents;
    assert_eq!(listed.len(), 1);
    server.stop().await;
}
