//! Signing in through a real identity provider: Keycloak with the realm in
//! `deploy/keycloak`. Runs when `RILL_TEST_KEYCLOAK` names the realm's
//! issuer (`http://localhost:8180/realms/rill`).

use std::collections::HashMap;

use rill_identity::keys::KeyServiceConfig;
use rill_proto::rill::v1::identity_service_client::IdentityServiceClient;
use rill_proto::rill::v1::session_service_client::SessionServiceClient;
use rill_runtime::config::IdentityConfig;
use rill_types::Principal;

use super::*;

/// Cookies by name, kept across a browser's requests to one site.
#[derive(Default)]
pub(super) struct Jar(HashMap<String, String>);

impl Jar {
    pub(super) fn take(&mut self, response: &reqwest::Response) {
        for value in response.headers().get_all("set-cookie") {
            if let Some((pair, _)) = value.to_str().unwrap_or_default().split_once(';') {
                if let Some((name, value)) = pair.split_once('=') {
                    self.0.insert(name.trim().into(), value.trim().into());
                }
            }
        }
    }

    pub(super) fn header(&self) -> String {
        self.0
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

pub(super) fn location(response: &reqwest::Response) -> String {
    response.headers()["location"].to_str().unwrap().to_string()
}

/// The `action` of the first form on a page.
pub(super) fn form_action(html: &str) -> String {
    html.split("action=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("a form")
        .replace("&amp;", "&")
}

#[tokio::test(flavor = "multi_thread")]
async fn people_sign_in_through_keycloak() {
    let Ok(issuer) = std::env::var("RILL_TEST_KEYCLOAK") else {
        eprintln!("RILL_TEST_KEYCLOAK not set; skipping");
        return;
    };
    let config = ServerConfig {
        identity: IdentityConfig {
            keys: KeyServiceConfig::Local {
                master_key_hex: Some("88".repeat(32)),
                master_key_file: None,
            },
            admins: vec!["root".into()],
            ..IdentityConfig::default()
        },
        ..ServerConfig::default()
    };
    let server = TestServer::start(config).await;
    let root = server.token_for(&Principal::human("root"));
    IdentityServiceClient::with_interceptor(connect(&server.grpc).await, Bearer(Some(root)))
        .put_identity_provider(pb::PutIdentityProviderRequest {
            operation_id: Bytes::new(),
            provider: Some(pb::IdentityProviderSpec {
                org: "default".into(),
                name: "keycloak".into(),
                issuer: issuer.clone(),
                client_id: "rill-web".into(),
                client_secret: Some("rill-web-secret".into()),
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
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    // The CLI's path: an ID token for the public client, exchanged.
    let tokens: serde_json::Value = http
        .post(format!("{issuer}/protocol/openid-connect/token"))
        .form(&[
            ("grant_type", "password"),
            ("client_id", "rill-cli"),
            ("username", "alice"),
            ("password", "alice"),
            ("scope", "openid profile email"),
        ])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let grant = SessionServiceClient::new(connect(&server.grpc).await)
        .exchange_token(pb::ExchangeTokenRequest {
            subject_token: tokens["id_token"].as_str().unwrap().into(),
            subject_token_type: "urn:ietf:params:oauth:token-type:id_token".into(),
            org: "default".into(),
            agent: None,
            model: None,
        })
        .await
        .unwrap()
        .into_inner();
    let me = clients(&server, Some(&grant.access_token))
        .await
        .admin
        .who_am_i(pb::WhoAmIRequest {})
        .await
        .unwrap()
        .into_inner();
    assert_eq!(me.principal.unwrap().name, "alice");
    assert_eq!(me.teams, ["devs"], "groups become teams");

    // The browser's path: the code flow through Keycloak's login form.
    let base = format!("http://{}", server.http);
    let to_keycloak = http
        .get(format!("{base}/auth/login?provider=keycloak"))
        .send()
        .await
        .unwrap();
    assert!(to_keycloak.status().is_redirection());
    let mut jar = Jar::default();
    let page = http.get(location(&to_keycloak)).send().await.unwrap();
    jar.take(&page);
    let action = form_action(&page.text().await.unwrap());
    let back = http
        .post(action)
        .header("cookie", jar.header())
        .form(&[
            ("username", "bob"),
            ("password", "bob"),
            ("credentialId", ""),
        ])
        .send()
        .await
        .unwrap();
    assert!(back.status().is_redirection(), "{}", back.status());
    let callback = url::Url::parse(&location(&back)).unwrap();
    let done = http
        .get(format!(
            "{base}/auth/callback?{}",
            callback.query().unwrap()
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(done.status(), 200);
    let cookie = done.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let granted: serde_json::Value = http
        .post(format!("{base}/auth/token"))
        .header("cookie", cookie)
        .header("x-rill-request", "1")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(granted["principal"], "bob");
    let me = clients(&server, Some(granted["access_token"].as_str().unwrap()))
        .await
        .admin
        .who_am_i(pb::WhoAmIRequest {})
        .await
        .unwrap()
        .into_inner();
    assert_eq!(me.principal.unwrap().name, "bob");
    server.stop().await;
}
