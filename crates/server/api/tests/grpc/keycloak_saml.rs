//! SAML 2.0 sign-in through Keycloak's `rill-saml` client. Needs the
//! `saml` feature and runs when `RILL_TEST_KEYCLOAK` names the realm's
//! issuer.

use base64::Engine;
use rill_identity::keys::KeyServiceConfig;
use rill_proto::rill::v1::IdentityServiceClient;
use rill_runtime::config::IdentityConfig;
use rill_types::Principal;

use super::keycloak::{form_action, location, Jar};
use super::*;

const PUBLIC_URL: &str = "http://localhost:7999";

/// A hidden input's value on Keycloak's auto-posting form.
fn hidden(html: &str, name: &str) -> String {
    html.split(&format!("name=\"{name}\""))
        .nth(1)
        .and_then(|rest| rest.split("value=\"").nth(1))
        .and_then(|rest| rest.split('"').next())
        .unwrap_or_else(|| panic!("no {name} on the page"))
        .to_string()
}

/// Start a sign-in at Rill, log in at Keycloak, and return what the
/// browser would post back: the ACS URL, SAMLResponse and RelayState.
async fn sign_in_at_idp(
    http: &reqwest::Client,
    base: &str,
    user: &str,
) -> (String, String, String) {
    let to_idp = http
        .get(format!("{base}/auth/login?provider=keycloak-saml"))
        .send()
        .await
        .unwrap();
    assert!(to_idp.status().is_redirection(), "{}", to_idp.status());
    let mut jar = Jar::default();
    let page = http.get(location(&to_idp)).send().await.unwrap();
    jar.take(&page);
    let action = form_action(&page.text().await.unwrap());
    let posted = http
        .post(action)
        .header("cookie", jar.header())
        .form(&[("username", user), ("password", user), ("credentialId", "")])
        .send()
        .await
        .unwrap();
    assert_eq!(posted.status(), 200);
    let html = posted.text().await.unwrap();
    (
        form_action(&html),
        hidden(&html, "SAMLResponse"),
        hidden(&html, "RelayState"),
    )
}

async fn post_acs(
    http: &reqwest::Client,
    acs: &str,
    response: &str,
    relay: &str,
) -> reqwest::Response {
    http.post(acs)
        .form(&[("SAMLResponse", response), ("RelayState", relay)])
        .send()
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn people_sign_in_through_keycloak_saml() {
    let Ok(issuer) = std::env::var("RILL_TEST_KEYCLOAK") else {
        eprintln!("RILL_TEST_KEYCLOAK not set; skipping");
        return;
    };
    let metadata = reqwest::get(format!("{issuer}/protocol/saml/descriptor"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let config = ServerConfig {
        identity: IdentityConfig {
            keys: KeyServiceConfig::Local {
                master_key_hex: Some("88".repeat(32)),
                master_key_file: None,
            },
            admins: vec!["root".into()],
            // As behind a proxy: the IdP sends browsers here, and the test
            // forwards to the server's real port.
            public_url: Some(PUBLIC_URL.into()),
            ..IdentityConfig::default()
        },
        ..ServerConfig::default()
    };
    let server = TestServer::start(config).await;
    let root = server.token_for(&Principal::human("root"));
    let saved = IdentityServiceClient::authed(connect(&server.grpc).await, Bearer(Some(root)))
        .update_identity_provider(pb::UpdateIdentityProviderRequest {
            request_id: String::new(),
            provider: (pb::IdentityProviderSpec {
                org: "default".into(),
                name: "keycloak-saml".into(),
                groups_claim: Some("groups".into()),
                auto_provision: true,
                saml_metadata: Some(metadata),
                saml_sp_entity_id: Some("rill-saml".into()),
                ..Default::default()
            })
            .into(),
            allow_missing: true,
        })
        .await
        .unwrap()
        .into_owned()
        .provider
        .unwrap();
    assert!(saved.saml);
    assert_eq!(saved.issuer, issuer, "the issuer is the IdP's entity id");

    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let base = format!("http://{}", server.http);

    // A response altered after signing is refused.
    let (acs, response, relay) = sign_in_at_idp(&http, &base, "bob").await;
    assert_eq!(acs, format!("{PUBLIC_URL}/auth/saml/acs"));
    let acs = format!("{base}/auth/saml/acs");
    let b64 = base64::engine::general_purpose::STANDARD;
    let xml = String::from_utf8(b64.decode(&response).unwrap()).unwrap();
    assert!(xml.contains(">bob<"), "the NameID is the username");
    let forged = b64.encode(xml.replace(">bob<", ">root<"));
    let refused = post_acs(&http, &acs, &forged, &relay).await;
    assert_eq!(refused.status(), 401);
    assert!(refused.text().await.unwrap().contains("must be signed"));

    // An intact one signs alice in, with her groups as teams, once.
    let (_, response, relay) = sign_in_at_idp(&http, &base, "alice").await;
    let done = post_acs(&http, &acs, &response, &relay).await;
    assert_eq!(done.status(), 200);
    let cookie = done.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    assert_eq!(
        post_acs(&http, &acs, &response, &relay).await.status(),
        401,
        "a response is good for one sign-in"
    );
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
    assert_eq!(granted["principal"], "alice");
    let me = clients(&server, Some(granted["access_token"].as_str().unwrap()))
        .await
        .identity
        .get_caller(pb::GetCallerRequest {})
        .await
        .unwrap()
        .into_owned();
    assert_eq!(me.principal.unwrap().name, "alice");
    assert_eq!(me.teams, ["devs"], "groups become teams");

    let sp_metadata = http
        .get(format!("{base}/auth/saml/metadata?provider=keycloak-saml"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(sp_metadata.contains("rill-saml"), "{sp_metadata}");
    server.stop().await;
}
