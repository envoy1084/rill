//! Agent budgets and the kill switch.

use rill_api::config::IdentityConfig;
use rill_identity::keys::KeyServiceConfig;
use rill_proto::rill::v1::identity_service_client::IdentityServiceClient;
use rill_types::identity::{AccountStatus, AgentBudgets, AgentRecord, Sponsor, UserRecord};
use rill_types::{Principal, Timestamp};

use super::*;

fn config() -> ServerConfig {
    ServerConfig {
        identity: IdentityConfig {
            keys: KeyServiceConfig::Local {
                master_key_hex: Some("55".repeat(32)),
                master_key_file: None,
            },
            admins: vec!["root".into()],
            ..IdentityConfig::default()
        },
        ..ServerConfig::default()
    }
}

async fn setup(stores: &rill_store::Stores, budgets: AgentBudgets) {
    stores.ensure_default_org().await.unwrap();
    let root = Principal::human("root");
    let now = Timestamp::now();
    stores
        .identity()
        .create_user(
            &UserRecord {
                org: "default".into(),
                principal: Principal::human("ann"),
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
            &root,
        )
        .await
        .unwrap();
    stores
        .identity()
        .create_agent(
            &AgentRecord {
                org: "default".into(),
                principal: Principal::agent("bot"),
                sponsor: Sponsor::User("ann".into()),
                purpose: "tests".into(),
                repos: vec!["*".into()],
                paths: vec![],
                models: vec![],
                budgets,
                workloads: vec![],
                status: AccountStatus::Active,
                expires_at: None,
                created_by: root.clone(),
                created_at: now,
                updated_at: now,
            },
            &root,
        )
        .await
        .unwrap();
}

/// A repository `r` the agent may write.
async fn repo(server: &TestServer) {
    let root = server.token_for(&Principal::human("root"));
    let mut c = clients(server, Some(&root)).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: "r".into(),
            description: None,
            windows_safe: false,
        })
        .await
        .unwrap();
    c.admin
        .add_relation(pb::AddRelationRequest {
            operation_id: Bytes::new(),
            relation: Some(pb::Relation {
                object: "repo:r".into(),
                relation: "writer".into(),
                subject: format!("principal:{}", Principal::agent("bot").id),
            }),
        })
        .await
        .unwrap();
}

fn edit(i: usize) -> pb::ApplyEditsRequest {
    pb::ApplyEditsRequest {
        operation_id: Bytes::new(),
        repo: "r".into(),
        base: None,
        edits: vec![set_edit(&format!("f{i}.txt"), b"x\n")],
        message: format!("v{i}"),
        intent: None,
        provenance: None,
        change_id: None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_daily_version_budget_is_never_exceeded() {
    let stores = rill_store::Stores::memory();
    setup(
        &stores,
        AgentBudgets {
            versions_per_day: Some(5),
            ..AgentBudgets::default()
        },
    )
    .await;
    let server = TestServer::start_on(config(), stores.clone()).await;
    repo(&server).await;
    let bot = server.token_for(&Principal::agent("bot"));
    let c = clients(&server, Some(&bot)).await;
    let writes = (0..12).map(|i| {
        let mut repo = c.repo.clone();
        async move { repo.apply_edits(edit(i)).await }
    });
    let results = futures::future::join_all(writes).await;
    let stored = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(stored, 5, "exactly the budget, however the writes race");
    assert!(results
        .iter()
        .filter_map(|r| r.as_ref().err())
        .all(|e| e.code() == Code::ResourceExhausted));
    server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_rate_budget_refuses_the_excess() {
    let stores = rill_store::Stores::memory();
    setup(
        &stores,
        AgentBudgets {
            requests_per_minute: Some(3),
            ..AgentBudgets::default()
        },
    )
    .await;
    let server = TestServer::start_on(config(), stores.clone()).await;
    let bot = server.token_for(&Principal::agent("bot"));
    let mut c = clients(&server, Some(&bot)).await;
    let mut codes = Vec::new();
    for _ in 0..5 {
        codes.push(
            c.admin
                .who_am_i(pb::WhoAmIRequest {})
                .await
                .err()
                .map(|s| s.code()),
        );
    }
    // A minute boundary may fall between the calls; at most two windows.
    let refused = codes
        .iter()
        .filter(|c| **c == Some(Code::ResourceExhausted))
        .count();
    assert!(refused >= 1, "{codes:?}");
    server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn killing_an_agent_ejects_its_queued_landing() {
    let stores = rill_store::Stores::memory();
    setup(&stores, AgentBudgets::default()).await;
    // Requests queue on a server without the lander.
    let mut api_only = config();
    api_only.roles.workers = false;
    let api = TestServer::start_on(api_only, stores.clone()).await;
    repo(&api).await;
    let bot = api.token_for(&Principal::agent("bot"));
    let root = api.token_for(&Principal::human("root"));
    let mut agent = clients(&api, Some(&bot)).await;
    let written = agent.repo.apply_edits(edit(0)).await.unwrap().into_inner();
    let change_id = written.version.unwrap().change_id;
    agent
        .review
        .submit_for_review(pb::SubmitForReviewRequest {
            operation_id: Bytes::new(),
            repo: "r".into(),
            change_id: change_id.clone(),
        })
        .await
        .unwrap();
    let mut human = clients(&api, Some(&root)).await;
    for (label, value) in [("Verified", 1), ("Review", 2)] {
        human
            .review
            .vote(pb::VoteRequest {
                operation_id: Bytes::new(),
                repo: "r".into(),
                change_id: change_id.clone(),
                label: label.into(),
                value,
                reason: None,
                version: None,
            })
            .await
            .unwrap();
    }
    let ticket = human
        .review
        .request_land(pb::RequestLandRequest {
            operation_id: Bytes::new(),
            repo: "r".into(),
            change_id: change_id.clone(),
        })
        .await
        .unwrap()
        .into_inner()
        .ticket
        .unwrap();

    let killed = IdentityServiceClient::with_interceptor(
        connect(&api.grpc).await,
        Bearer(Some(root.clone())),
    )
    .kill_switch(pb::KillSwitchRequest {
        operation_id: Bytes::new(),
        org: "default".into(),
        target: Some(pb::kill_switch_request::Target::Agent("bot".into())),
        reason: "misbehaving".into(),
    })
    .await
    .unwrap()
    .into_inner();
    assert_eq!(killed.agents_suspended, ["bot"]);
    assert_eq!(
        agent
            .admin
            .who_am_i(pb::WhoAmIRequest {})
            .await
            .unwrap_err()
            .code(),
        Code::Unauthenticated
    );

    // A server with the lander picks the request up and ejects it.
    let workers = TestServer::start_on(config(), stores.clone()).await;
    let mut human = clients(&workers, Some(&root)).await;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let status = loop {
        let status = human
            .review
            .get_land_status(pb::GetLandStatusRequest {
                repo: "r".into(),
                ticket: ticket.id.clone(),
            })
            .await
            .unwrap()
            .into_inner();
        let code = pb::LandStatus::try_from(status.ticket.as_ref().unwrap().status).unwrap();
        if !matches!(code, pb::LandStatus::Queued | pb::LandStatus::Rebasing) {
            break status;
        }
        assert!(tokio::time::Instant::now() < deadline, "never decided");
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_ne!(
        pb::LandStatus::try_from(status.ticket.unwrap().status).unwrap(),
        pb::LandStatus::Landed
    );
    assert!(
        status
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("author"),
        "{:?}",
        status.error
    );
    api.stop().await;
    workers.stop().await;
}
