//! Agent budgets and the kill switch.

use rill_identity::keys::KeyServiceConfig;
use rill_proto::rill::v1::IdentityServiceClient;
use rill_runtime::config::IdentityConfig;
use rill_types::identity::AgentBudgets;
use rill_types::Principal;

use super::*;
use crate::common::{names, register_bot};

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

/// A repository `r` the agent may write.
async fn repo(server: &TestServer) {
    let root = server.token_for(&Principal::human("root"));
    let c = clients(server, Some(&root)).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            parent: names::org("default"),
            repo_id: "r".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    c.access
        .create_relation(pb::CreateRelationRequest {
            request_id: String::new(),
            parent: names::repo("r"),
            relation: (pb::Relation {
                path: String::new(),
                relation: "writer".into(),
                subject: format!("principal:{}", Principal::agent("bot").id),
            })
            .into(),
        })
        .await
        .unwrap();
}

fn edit(i: usize) -> pb::ApplyEditsRequest {
    pb::ApplyEditsRequest {
        parent: names::repo("r"),
        edits: vec![set_edit(&format!("f{i}.txt"), b"x\n")],
        message: format!("v{i}"),
        ..Default::default()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_daily_version_budget_is_never_exceeded() {
    let stores = rill_store::Stores::memory();
    register_bot(
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
        let changes = c.changes.clone();
        async move { changes.apply_edits(edit(i)).await }
    });
    let results = futures::future::join_all(writes).await;
    let stored = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(stored, 5, "exactly the budget, however the writes race");
    assert!(results
        .iter()
        .filter_map(|r| r.as_ref().err())
        .all(|e| e.code == Code::ResourceExhausted));
    server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_rate_budget_refuses_the_excess() {
    let stores = rill_store::Stores::memory();
    register_bot(
        &stores,
        AgentBudgets {
            requests_per_minute: Some(3),
            ..AgentBudgets::default()
        },
    )
    .await;
    let server = TestServer::start_on(config(), stores.clone()).await;
    let bot = server.token_for(&Principal::agent("bot"));
    let c = clients(&server, Some(&bot)).await;
    let mut codes = Vec::new();
    for _ in 0..5 {
        codes.push(
            c.identity
                .get_caller(pb::GetCallerRequest {})
                .await
                .err()
                .map(|s| s.code),
        );
    }
    let refused = codes
        .iter()
        .filter(|c| **c == Some(Code::ResourceExhausted))
        .count();
    assert_eq!(
        refused, 2,
        "a burst of the budget, then refusals: {codes:?}"
    );
    server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn killing_an_agent_ejects_its_queued_landing() {
    let stores = rill_store::Stores::memory();
    register_bot(&stores, AgentBudgets::default()).await;
    // Requests queue on a server without the lander.
    let mut api_only = config();
    api_only.roles.workers = false;
    let api = TestServer::start_on(api_only, stores.clone()).await;
    repo(&api).await;
    let bot = api.token_for(&Principal::agent("bot"));
    let root = api.token_for(&Principal::human("root"));
    let agent = clients(&api, Some(&bot)).await;
    let written = agent
        .changes
        .apply_edits(edit(0))
        .await
        .unwrap()
        .into_owned();
    let change_id = written.version.unwrap().change_id;
    agent
        .review
        .submit_change(pb::SubmitChangeRequest {
            request_id: String::new(),
            change: names::change("r", &change_id),
        })
        .await
        .unwrap();
    let human = clients(&api, Some(&root)).await;
    for (label, value) in [("Verified", 1), ("Review", 2)] {
        human
            .review
            .vote(pb::VoteRequest {
                request_id: String::new(),
                review: names::review("r", &change_id),
                label: label.into(),
                value,
                reason: None,
                version: String::new(),
            })
            .await
            .unwrap();
    }
    let ticket = human
        .landing
        .request_land(pb::RequestLandRequest {
            request_id: String::new(),
            change: names::change("r", &change_id),
            onto: String::new(),
        })
        .await
        .unwrap()
        .into_owned()
        .land_request
        .unwrap();

    let killed =
        IdentityServiceClient::authed(connect(&api.grpc).await, Bearer(Some(root.clone())))
            .kill_switch(pb::KillSwitchRequest {
                request_id: String::new(),
                parent: names::org("default"),
                target: Some(pb::kill_switch_request::Target::Agent("bot".into())),
                reason: "misbehaving".into(),
            })
            .await
            .unwrap()
            .into_owned();
    assert_eq!(killed.agents_suspended, ["bot"]);
    assert_eq!(
        agent
            .identity
            .get_caller(pb::GetCallerRequest {})
            .await
            .unwrap_err()
            .code,
        Code::Unauthenticated
    );

    // A server with the lander picks the request up and ejects it.
    let workers = TestServer::start_on(config(), stores.clone()).await;
    let human = clients(&workers, Some(&root)).await;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let status = loop {
        let status = human
            .landing
            .get_land_request(pb::GetLandRequestRequest {
                name: ticket.name.clone(),
            })
            .await
            .unwrap()
            .into_owned()
            .land_request
            .unwrap();
        let code = rill_proto::convert::known::<pb::LandStatus>(status.status).unwrap();
        if !matches!(code, pb::LandStatus::Queued | pb::LandStatus::Rebasing) {
            break status;
        }
        assert!(tokio::time::Instant::now() < deadline, "never decided");
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_ne!(
        rill_proto::convert::known::<pb::LandStatus>(status.status).unwrap(),
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
