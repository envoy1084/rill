//! A real Kubernetes service account token exchanged for an agent token.
//! Runs when `RILL_TEST_K8S_DIR` names a directory holding the cluster's
//! `issuer.txt`, `jwks.json`, a `token.jwt` for service account
//! `rill-test/builder` with audience `rill`, and a `token-other.jwt` with
//! another audience (see `architecture/09-engineering/testing.md`).

use rill_api::config::IdentityConfig;
use rill_identity::keys::KeyServiceConfig;
use rill_proto::rill::v1::identity_service_client::IdentityServiceClient;
use rill_proto::rill::v1::session_service_client::SessionServiceClient;
use rill_types::identity::{AccountStatus, UserRecord};
use rill_types::{Principal, Timestamp};

use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn a_kubernetes_service_account_acts_as_its_agent() {
    let Ok(dir) = std::env::var("RILL_TEST_K8S_DIR") else {
        eprintln!("RILL_TEST_K8S_DIR not set; skipping");
        return;
    };
    let read = |name: &str| {
        std::fs::read_to_string(format!("{dir}/{name}"))
            .unwrap()
            .trim()
            .to_string()
    };
    let (issuer, jwks, token, other) = (
        read("issuer.txt"),
        read("jwks.json"),
        read("token.jwt"),
        read("token-other.jwt"),
    );

    let stores = rill_store::Stores::memory();
    stores.ensure_default_org().await.unwrap();
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
                created_at: now,
                updated_at: now,
            },
            &Principal::human("root"),
        )
        .await
        .unwrap();
    let config = ServerConfig {
        identity: IdentityConfig {
            keys: KeyServiceConfig::Local {
                master_key_hex: Some("66".repeat(32)),
                master_key_file: None,
            },
            admins: vec!["root".into()],
            ..IdentityConfig::default()
        },
        ..ServerConfig::default()
    };
    let server = TestServer::start_on(config, stores).await;
    let root = server.token_for(&Principal::human("root"));
    let mut admin =
        IdentityServiceClient::with_interceptor(connect(&server.grpc).await, Bearer(Some(root)));
    admin
        .put_workload_issuer(pb::PutWorkloadIssuerRequest {
            operation_id: Bytes::new(),
            issuer: Some(pb::WorkloadIssuerSpec {
                org: "default".into(),
                name: "cluster".into(),
                issuer: issuer.clone(),
                keys: Some(pb::workload_issuer_spec::Keys::Jwks(jwks)),
                audiences: vec!["rill".into()],
            }),
        })
        .await
        .unwrap();
    admin
        .register_agent(pb::RegisterAgentRequest {
            operation_id: Bytes::new(),
            agent: Some(pb::AgentSpec {
                org: "default".into(),
                name: "builder".into(),
                sponsor: "user:ann".into(),
                purpose: "kubernetes".into(),
                repos: vec!["*".into()],
                paths: vec![],
                models: vec![],
                budgets: None,
                workloads: vec![pb::WorkloadBinding {
                    issuer,
                    subject: "system:serviceaccount:rill-test:*".into(),
                    claims: Default::default(),
                }],
                expires_at: None,
            }),
        })
        .await
        .unwrap();
    let exchange = |jwt: String| {
        let grpc = server.grpc.clone();
        async move {
            SessionServiceClient::new(connect(&grpc).await)
                .exchange_token(pb::ExchangeTokenRequest {
                    subject_token: jwt,
                    subject_token_type: "urn:ietf:params:oauth:token-type:jwt".into(),
                    org: "default".into(),
                    agent: None,
                    model: None,
                })
                .await
        }
    };
    let grant = exchange(token).await.unwrap().into_inner();
    let me = clients(&server, Some(&grant.access_token))
        .await
        .admin
        .who_am_i(pb::WhoAmIRequest {})
        .await
        .unwrap()
        .into_inner();
    assert_eq!(me.principal.unwrap().name, "builder");
    assert_eq!(me.sponsor.as_deref(), Some("user:ann"));
    assert!(
        exchange(other).await.is_err(),
        "a token for another audience is refused"
    );
    server.stop().await;
}
