//! Agents: registration, the workload exchange, registered scope, and
//! suspension of the agent or its sponsor.

use rill_identity::keys::KeyServiceConfig;
use rill_identity::testing::MockIdp;
use rill_proto::rill::v1::IdentityServiceClient;
use rill_proto::rill::v1::SessionServiceClient;
use rill_runtime::config::IdentityConfig;
use rill_types::identity::{AccountStatus, OrgIdentitySettings, UserRecord};
use rill_types::{Principal, Timestamp};

use super::*;
use common::names;

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
    IdentityServiceClient::authed(connect(&server.grpc).await, Bearer(Some(token.into())))
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
        external_id: None,
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
) -> Result<pb::SessionGrant, ConnectError> {
    SessionServiceClient::authed(connect(&server.grpc).await, Bearer(None))
        .exchange_token(pb::ExchangeTokenRequest {
            subject_token: jwt.into(),
            subject_token_type: "urn:ietf:params:oauth:token-type:jwt".into(),
            org: "default".into(),
            agent: None,
            model: model.map(String::from),
        })
        .await
        .map(|r| r.into_owned().grant.unwrap())
}

async fn refused_within(server: &TestServer, token: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let c = clients(server, Some(token)).await;
        match c.identity.get_caller(pb::GetCallerRequest {}).await {
            Err(s) if s.code == Code::Unauthenticated => return,
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
        budgets: None.into(),
        workloads: vec![pb::WorkloadBinding {
            issuer: issuer.into(),
            subject: format!("ci:{name}"),
            claims: Default::default(),
        }],
        expires_at: None,
        reviewer: None.into(),
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
    let admin = identity(&a, &root).await;
    admin
        .update_workload_issuer(pb::UpdateWorkloadIssuerRequest {
            request_id: String::new(),
            issuer: (pb::WorkloadIssuerSpec {
                resource_name: String::new(),
                org: "default".into(),
                name: "ci".into(),
                issuer: idp.issuer().into(),
                keys: Some(pb::workload_issuer_spec::Keys::Discovery(true)),
                audiences: vec!["rill".into()],
            })
            .into(),
            allow_missing: true,
        })
        .await
        .unwrap();
    admin
        .create_agent(pb::CreateAgentRequest {
            request_id: String::new(),
            parent: names::org("default"),
            agent: (spec("builder", "user:ann", idp.issuer())).into(),
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
    let session = grant.session.clone();
    let token = grant.access_token;
    let me = clients(&b, Some(&token))
        .await
        .identity
        .get_caller(pb::GetCallerRequest {})
        .await
        .unwrap()
        .into_owned();
    assert_eq!(me.principal.unwrap().name, "builder");
    assert_eq!(me.sponsor.as_deref(), Some("user:ann"));
    assert_eq!(me.model.as_deref(), Some("model-a"));
    assert!(exchange(&a, &workload_jwt(&idp, "ci:stranger"), None)
        .await
        .is_err());

    // Relations would let the agent write anywhere; its registration does not.
    let root_clients = clients(&a, Some(&root)).await;
    for repo in ["allowed", "other"] {
        root_clients
            .repo
            .create_repo(pb::CreateRepoRequest {
                request_id: String::new(),
                parent: names::org("default"),
                repo_id: repo.into(),
                repo: None.into(),
            })
            .await
            .unwrap();
        root_clients
            .access
            .create_relation(pb::CreateRelationRequest {
                request_id: String::new(),
                parent: names::repo(repo),
                relation: (pb::Relation {
                    path: String::new(),
                    relation: "writer".into(),
                    subject: format!("principal:{}", Principal::agent("builder").id),
                })
                .into(),
            })
            .await
            .unwrap();
    }
    let agent = clients(&b, Some(&token)).await;
    let write = |repo: &str, path: &str| pb::ApplyEditsRequest {
        request_id: String::new(),
        parent: names::repo(repo),
        base: None.into(),
        edits: vec![set_edit(path, b"x\n")],
        message: "m".into(),
        intent: String::new(),
        provenance: None.into(),
        change: String::new(),
    };
    let written = agent
        .changes
        .apply_edits(write("allowed", "src/a.rs"))
        .await
        .unwrap()
        .into_owned();
    // The server, not the client, says which session, sponsor and model wrote it.
    let provenance = written.version.unwrap().provenance.unwrap();
    assert_eq!(provenance.session.as_deref(), Some(session.as_str()));
    assert_eq!(provenance.model.as_deref(), Some("model-a"));
    assert_eq!(provenance.sponsor.unwrap().name, "ann");
    let outside = agent
        .changes
        .apply_edits(write("allowed", "docs/a.md"))
        .await
        .unwrap_err();
    assert_eq!(outside.code, Code::PermissionDenied);
    let other = agent
        .changes
        .apply_edits(write("other", "src/a.rs"))
        .await
        .unwrap_err();
    assert_eq!(other.code, Code::PermissionDenied);

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
            request_id: String::new(),
            agent: (pb::Agent {
                name: "orgs/default/agents/builder".into(),
                status: "suspended".into(),
                ..Default::default()
            })
            .into(),
            update_mask: (rill_proto::FieldMask {
                paths: vec!["status".into()],
                ..Default::default()
            })
            .into(),
            etag: String::new(),
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
    let svc = identity(&server, &ann).await;
    svc.create_agent(pb::CreateAgentRequest {
        request_id: String::new(),
        parent: names::org("default"),
        agent: (spec("anns-bot", "user:ann", "https://ci")).into(),
    })
    .await
    .unwrap();
    let denied = svc
        .create_agent(pb::CreateAgentRequest {
            request_id: String::new(),
            parent: names::org("default"),
            agent: (spec("bobs-bot", "user:bob", "https://ci")).into(),
        })
        .await
        .unwrap_err();
    assert_eq!(denied.code, Code::PermissionDenied);
    let listed = svc
        .list_agents(pb::ListAgentsRequest {
            parent: names::org("default"),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .agents;
    assert_eq!(listed.len(), 1);
    server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn tokens_for_others_go_only_to_agents_one_sponsors() {
    let stores = rill_store::Stores::memory();
    stores.ensure_default_org().await.unwrap();
    for name in ["ann", "mallory"] {
        stores
            .identity()
            .create_user(&user(name), &Principal::human("root"))
            .await
            .unwrap();
    }
    let server = TestServer::start_on(config(), stores.clone()).await;
    let ann = server.token_for(&Principal::human("ann"));
    identity(&server, &ann)
        .await
        .create_agent(pb::CreateAgentRequest {
            request_id: String::new(),
            parent: names::org("default"),
            agent: (pb::AgentSpec {
                repos: vec!["*".into()],
                paths: vec![],
                ..spec("anns-bot", "user:ann", "https://ci")
            })
            .into(),
        })
        .await
        .unwrap();
    let issue = |owner: String| {
        let grpc = server.grpc.clone();
        async move {
            let c = clients_at(&grpc, Some(&owner)).await;
            let repo = format!("repo-of-{}", &owner[owner.len() - 6..]);
            c.repo
                .create_repo(pb::CreateRepoRequest {
                    request_id: String::new(),
                    parent: names::org("default"),
                    repo_id: repo.clone(),
                    repo: None.into(),
                })
                .await
                .unwrap();
            c.identity
                .issue_token(pb::IssueTokenRequest {
                    request_id: String::new(),
                    capabilities: vec![pb::Capability {
                        action: "read".into(),
                        scope: format!("repo:{repo}"),
                        expires_at: None,
                    }],
                    expires_at: None,
                    for_principal: (pb::Principal::from(&Principal::agent("anns-bot"))).into(),
                })
                .await
        }
    };
    // Mallory owns her repository but does not sponsor the agent.
    let mallory = server.token_for(&Principal::human("mallory"));
    assert_eq!(
        issue(mallory).await.unwrap_err().code,
        Code::PermissionDenied
    );
    assert!(issue(ann).await.is_ok());
    server.stop().await;
}
