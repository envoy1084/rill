//! Signing in through an organization's identity provider: the browser
//! flow, the CLI's token exchange, refresh, logout and suspension.

use std::collections::HashMap;

use rill_api::config::IdentityConfig;
use rill_identity::keys::KeyServiceConfig;
use rill_identity::testing::{MockIdp, MockUser};
use rill_proto::rill::v1::identity_service_client::IdentityServiceClient;
use rill_types::Principal;

use super::*;

fn config() -> ServerConfig {
    ServerConfig {
        identity: IdentityConfig {
            keys: KeyServiceConfig::Local {
                master_key_hex: Some("22".repeat(32)),
                master_key_file: None,
            },
            admins: vec!["root".into()],
            ..IdentityConfig::default()
        },
        ..ServerConfig::default()
    }
}

async fn identity(server: &TestServer, token: &str) -> IdentityServiceClient<Svc> {
    IdentityServiceClient::with_interceptor(
        connect(&server.grpc).await,
        Bearer(Some(token.to_string())),
    )
}

/// A provider `mock` in the default org, with a confidential client for
/// browsers and a public one for the CLI.
async fn register(server: &TestServer, idp: &MockIdp) {
    idp.add_client("rill-web", Some("web-secret"));
    idp.add_client("rill-cli", None);
    let root = server.token_for(&Principal::human("root"));
    identity(server, &root)
        .await
        .put_identity_provider(pb::PutIdentityProviderRequest {
            operation_id: Bytes::new(),
            provider: Some(pb::IdentityProviderSpec {
                org: "default".into(),
                name: "mock".into(),
                issuer: idp.issuer().into(),
                client_id: "rill-web".into(),
                client_secret: Some("web-secret".into()),
                cli_client_id: Some("rill-cli".into()),
                scopes: vec![],
                username_claim: None,
                groups_claim: Some("groups".into()),
                auto_provision: true,
                saml_metadata: None,
                saml_sp_entity_id: None,
            }),
        })
        .await
        .unwrap();
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

fn location(response: &reqwest::Response) -> url::Url {
    url::Url::parse(response.headers()["location"].to_str().unwrap()).unwrap()
}

/// Run the browser flow; returns the session cookie.
async fn browser_sign_in(server: &TestServer, idp: &MockIdp, user: &str) -> String {
    idp.sign_in_as(user);
    let base = format!("http://{}", server.http);
    let client = http();
    let to_idp = client
        .get(format!("{base}/auth/login?provider=mock"))
        .send()
        .await
        .unwrap();
    assert!(to_idp.status().is_redirection(), "{}", to_idp.status());
    let back = client.get(location(&to_idp)).send().await.unwrap();
    let query = location(&back).query().unwrap().to_string();
    let done = client
        .get(format!("{base}/auth/callback?{query}"))
        .send()
        .await
        .unwrap();
    assert_eq!(done.status(), 200, "{}", done.text().await.unwrap());
    let cookie = done.headers()["set-cookie"].to_str().unwrap();
    assert!(cookie.contains("HttpOnly"));
    cookie.split(';').next().unwrap().to_string()
}

async fn access_from_cookie(server: &TestServer, cookie: &str) -> reqwest::Response {
    http()
        .post(format!("http://{}/auth/token", server.http))
        .header("cookie", cookie)
        .header("x-rill-request", "1")
        .send()
        .await
        .unwrap()
}

async fn who(server: &TestServer, token: &str) -> Result<pb::WhoAmIResponse, Status> {
    clients(server, Some(token))
        .await
        .admin
        .who_am_i(pb::WhoAmIRequest {})
        .await
        .map(|r| r.into_inner())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_browser_signs_in_through_the_provider() {
    let idp = MockIdp::start().await;
    idp.add_user(MockUser::new("alice").in_groups(&["core"]));
    let server = TestServer::start(config()).await;
    register(&server, &idp).await;

    let cookie = browser_sign_in(&server, &idp, "alice").await;
    // The page's token request needs the CSRF header.
    let no_header = http()
        .post(format!("http://{}/auth/token", server.http))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(no_header.status(), 401);

    let granted = access_from_cookie(&server, &cookie).await;
    assert_eq!(granted.status(), 200);
    let rotated = granted.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let body: serde_json::Value = granted.json().await.unwrap();
    let access = body["access_token"].as_str().unwrap().to_string();
    let me = who(&server, &access).await.unwrap();
    assert_eq!(me.principal.unwrap().name, "alice");
    assert_eq!(me.teams, ["core"]);
    assert!(me.session.is_some());

    // The cookie rotated: the old one is spent.
    assert_eq!(access_from_cookie(&server, &cookie).await.status(), 401);

    // Logging out ends the session, and with it the access token.
    let out = http()
        .post(format!("http://{}/auth/logout", server.http))
        .header("cookie", &rotated)
        .header("x-rill-request", "1")
        .send()
        .await
        .unwrap();
    assert_eq!(out.status(), 204);
    assert_eq!(
        who(&server, &access).await.unwrap_err().code(),
        Code::Unauthenticated
    );
    assert_eq!(access_from_cookie(&server, &rotated).await.status(), 401);

    server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_cli_exchanges_an_id_token_and_refreshes() {
    let idp = MockIdp::start().await;
    idp.add_user(MockUser::new("bob"));
    let server = TestServer::start(config()).await;
    register(&server, &idp).await;
    let token_url = format!("http://{}/oauth/token", server.http);

    let id_token = idp.id_token("bob", "rill-cli", None);
    let granted: serde_json::Value = http()
        .post(&token_url)
        .form(&[
            (
                "grant_type",
                "urn:ietf:params:oauth:grant-type:token-exchange",
            ),
            ("subject_token", id_token.as_str()),
            (
                "subject_token_type",
                "urn:ietf:params:oauth:token-type:id_token",
            ),
        ])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let access = granted["access_token"].as_str().unwrap();
    assert_eq!(
        who(&server, access).await.unwrap().principal.unwrap().name,
        "bob"
    );

    let refresh = granted["refresh_token"].as_str().unwrap().to_string();
    let refreshed = http()
        .post(&token_url)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh.as_str()),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(refreshed.status(), 200);
    let again = http()
        .post(&token_url)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh.as_str()),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(again.status(), 400, "a refresh token works once");
    let body: serde_json::Value = again.json().await.unwrap();
    assert_eq!(body["error"], "invalid_grant");

    // An ID token for an audience the provider does not list is refused.
    let foreign = idp.id_token("bob", "someone-else", None);
    let refused = http()
        .post(&token_url)
        .form(&[
            (
                "grant_type",
                "urn:ietf:params:oauth:grant-type:token-exchange",
            ),
            ("subject_token", foreign.as_str()),
            (
                "subject_token_type",
                "urn:ietf:params:oauth:token-type:id_token",
            ),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), 400);
    server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn suspending_a_user_refuses_their_tokens_everywhere() {
    let idp = MockIdp::start().await;
    idp.add_user(MockUser::new("carol"));
    let stores = rill_store::Stores::memory();
    stores.ensure_default_org().await.unwrap();
    let a = TestServer::start_on(config(), stores.clone()).await;
    let b = TestServer::start_on(config(), stores.clone()).await;
    register(&a, &idp).await;
    let cookie = browser_sign_in(&a, &idp, "carol").await;
    let body: serde_json::Value = access_from_cookie(&a, &cookie).await.json().await.unwrap();
    let access = body["access_token"].as_str().unwrap().to_string();
    assert!(who(&b, &access).await.is_ok());

    let root = a.token_for(&Principal::human("root"));
    identity(&a, &root)
        .await
        .update_user(pb::UpdateUserRequest {
            operation_id: Bytes::new(),
            org: "default".into(),
            name: "carol".into(),
            suspended: Some(true),
            set_roles: false,
            roles: vec![],
        })
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while who(&b, &access).await.is_ok() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "suspension never reached b"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        who(&b, &access).await.unwrap_err().code(),
        Code::Unauthenticated
    );
    assert_eq!(access_from_cookie(&a, &cookie).await.status(), 401);
    a.stop().await;
    b.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn org_admins_manage_only_their_org() {
    let idp = MockIdp::start().await;
    idp.add_user(MockUser::new("dana"));
    let server = TestServer::start(config()).await;
    register(&server, &idp).await;
    let cookie = browser_sign_in(&server, &idp, "dana").await;
    let mut tokens: HashMap<&str, String> = HashMap::new();
    let body: serde_json::Value = access_from_cookie(&server, &cookie)
        .await
        .json()
        .await
        .unwrap();
    tokens.insert("dana", body["access_token"].as_str().unwrap().into());
    let list = |org: &str| pb::ListIdentityProvidersRequest { org: org.into() };

    let denied = identity(&server, &tokens["dana"])
        .await
        .list_identity_providers(list("default"))
        .await
        .unwrap_err();
    assert_eq!(denied.code(), Code::PermissionDenied);

    let root = server.token_for(&Principal::human("root"));
    identity(&server, &root)
        .await
        .update_user(pb::UpdateUserRequest {
            operation_id: Bytes::new(),
            org: "default".into(),
            name: "dana".into(),
            suspended: None,
            set_roles: true,
            roles: vec!["admin".into()],
        })
        .await
        .unwrap();
    let mut dana = identity(&server, &tokens["dana"]).await;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let listed = loop {
        match dana.list_identity_providers(list("default")).await {
            Ok(r) => break r.into_inner(),
            Err(_) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(50)).await
            }
            Err(e) => panic!("{e}"),
        }
    };
    assert_eq!(listed.providers.len(), 1);
    assert!(listed.providers[0].has_client_secret);
    let other = dana
        .list_identity_providers(list("acme"))
        .await
        .unwrap_err();
    assert_eq!(other.code(), Code::PermissionDenied);
    server.stop().await;
}
