//! SCIM provisioning over HTTP: credentials, users, groups as teams in
//! relation checks, and deprovisioning.

use rill_identity::keys::KeyServiceConfig;
use rill_proto::rill::v1::IdentityServiceClient;
use rill_runtime::config::IdentityConfig;
use rill_types::Principal;
use serde_json::{json, Value};

use super::*;

fn config() -> ServerConfig {
    ServerConfig {
        identity: IdentityConfig {
            keys: KeyServiceConfig::Local {
                master_key_hex: Some("77".repeat(32)),
                master_key_file: None,
            },
            admins: vec!["root".into()],
            ..IdentityConfig::default()
        },
        ..ServerConfig::default()
    }
}

struct ScimClient {
    base: String,
    token: String,
    http: reqwest::Client,
}

impl ScimClient {
    async fn send(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> (u16, Value) {
        let mut request = self
            .http
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&self.token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let body = response.json().await.unwrap_or(Value::Null);
        (status, body)
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_provisioning_system_manages_users_and_teams() {
    let stores = rill_store::Stores::memory();
    stores.ensure_default_org().await.unwrap();
    let a = TestServer::start_on(config(), stores.clone()).await;
    let b = TestServer::start_on(config(), stores.clone()).await;
    let root = a.token_for(&Principal::human("root"));
    let identity =
        IdentityServiceClient::authed(connect(&a.grpc).await, Bearer(Some(root.clone())));
    let created = identity
        .create_scim_credential(pb::CreateScimCredentialRequest {
            request_id: String::new(),
            parent: "orgs/default".into(),
        })
        .await
        .unwrap()
        .into_owned();
    let scim = ScimClient {
        base: format!("http://{}/scim/v2/default", a.http),
        token: created.token.clone(),
        http: reqwest::Client::new(),
    };

    let (status, _) = ScimClient {
        token: "scim_x.y".into(),
        ..scim_clone(&scim)
    }
    .send(reqwest::Method::GET, "/Users", None)
    .await;
    assert_eq!(status, 401);
    let (status, _) = ScimClient {
        base: format!("http://{}/scim/v2/other", a.http),
        ..scim_clone(&scim)
    }
    .send(reqwest::Method::GET, "/Users", None)
    .await;
    assert_eq!(status, 401, "a credential works only for its org");
    let (status, config_body) = scim
        .send(reqwest::Method::GET, "/ServiceProviderConfig", None)
        .await;
    assert_eq!(status, 200);
    assert_eq!(config_body["patch"]["supported"], true);

    let (status, ann) = scim
        .send(
            reqwest::Method::POST,
            "/Users",
            Some(json!({"schemas": [rill_identity::scim::USER_SCHEMA], "userName": "ann", "active": true})),
        )
        .await;
    assert_eq!(status, 201, "{ann}");
    let ann_id = ann["id"].as_str().unwrap().to_string();

    // A relation granted to a team opens the repository once SCIM puts the
    // user in the group.
    let admin = clients(&a, Some(&root)).await;
    admin
        .repo
        .create_repo(pb::CreateRepoRequest {
            request_id: String::new(),
            parent: "orgs/default".into(),
            repo_id: "r".into(),
            repo: None.into(),
        })
        .await
        .unwrap();
    admin
        .access
        .create_relation(pb::CreateRelationRequest {
            request_id: String::new(),
            parent: common::names::repo("r"),
            relation: (pb::Relation {
                path: String::new(),
                relation: "reader".into(),
                subject: "team:devs".into(),
            })
            .into(),
        })
        .await
        .unwrap();
    let ann_token = a.token_for(&Principal::human("ann"));
    let list = || pb::ListDirectoryRequest {
        repo: common::names::repo("r"),
        ..Default::default()
    };
    let denied = clients(&b, Some(&ann_token))
        .await
        .content
        .list_directory(list())
        .await
        .unwrap_err();
    assert_eq!(denied.code, Code::PermissionDenied);
    let (status, _) = scim
        .send(
            reqwest::Method::POST,
            "/Groups",
            Some(json!({"displayName": "devs", "members": [{"value": ann_id}]})),
        )
        .await;
    assert_eq!(status, 201);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        match clients(&b, Some(&ann_token))
            .await
            .content
            .list_directory(list())
            .await
        {
            Ok(_) => break,
            Err(_) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(50)).await
            }
            Err(e) => panic!("team access never arrived: {e}"),
        }
    }

    // Deactivation refuses the user's tokens on every server.
    let (status, _) = scim
        .send(
            reqwest::Method::PATCH,
            &format!("/Users/{ann_id}"),
            Some(json!({"schemas": [rill_identity::scim::PATCH_SCHEMA],
                        "Operations": [{"op": "replace", "value": {"active": false}}]})),
        )
        .await;
    assert_eq!(status, 200);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let refused = clients(&b, Some(&ann_token))
            .await
            .identity
            .get_caller(pb::GetCallerRequest {})
            .await
            .is_err();
        if refused {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "never refused");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    identity
        .delete_scim_credential(pb::DeleteScimCredentialRequest {
            request_id: String::new(),
            name: created.credential.unwrap().name,
        })
        .await
        .unwrap();
    let (status, _) = scim.send(reqwest::Method::GET, "/Users", None).await;
    assert_eq!(status, 401, "a revoked credential is refused");
    a.stop().await;
    b.stop().await;
}

fn scim_clone(c: &ScimClient) -> ScimClient {
    ScimClient {
        base: c.base.clone(),
        token: c.token.clone(),
        http: c.http.clone(),
    }
}
