//! The SDK's device flow against a real identity provider: Keycloak with
//! the realm in `deploy/keycloak`, the code approved through Keycloak's own
//! pages as a person would. Runs when `RILL_TEST_KEYCLOAK` names the
//! realm's issuer.

use std::collections::HashMap;

use rill_sdk::auth::DeviceFlow;

#[derive(Default)]
struct Jar(HashMap<String, String>);

impl Jar {
    fn take(&mut self, response: &reqwest::Response) {
        for value in response.headers().get_all("set-cookie") {
            if let Some((pair, _)) = value.to_str().unwrap_or_default().split_once(';') {
                if let Some((name, value)) = pair.split_once('=') {
                    self.0.insert(name.trim().into(), value.trim().into());
                }
            }
        }
    }

    fn header(&self) -> String {
        self.0
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

fn form_action(html: &str) -> Option<String> {
    html.split("action=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .map(|a| a.replace("&amp;", "&"))
}

/// Open `url` and log in as `user`, following Keycloak's redirects.
async fn approve(url: &str, user: &str) {
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let mut jar = Jar::default();
    let mut next = url.to_string();
    let mut method_post: Option<Vec<(String, String)>> = None;
    for _ in 0..10 {
        let request = match method_post.take() {
            Some(form) => http.post(&next).form(&form),
            None => http.get(&next),
        };
        let response = request.header("cookie", jar.header()).send().await.unwrap();
        jar.take(&response);
        if response.status().is_redirection() {
            let to = response.headers()["location"].to_str().unwrap().to_string();
            next = url::Url::parse(&next)
                .unwrap()
                .join(&to)
                .unwrap()
                .to_string();
            continue;
        }
        let html = response.text().await.unwrap();
        let here = url::Url::parse(&next).unwrap();
        let resolve = |action: String| here.join(&action).unwrap().to_string();
        if html.contains("name=\"password\"") {
            next = resolve(form_action(&html).expect("a login form"));
            method_post = Some(vec![
                ("username".into(), user.into()),
                ("password".into(), user.into()),
                ("credentialId".into(), String::new()),
            ]);
        } else if html.contains("name=\"accept\"") {
            next = resolve(form_action(&html).expect("a consent form"));
            method_post = Some(vec![("accept".into(), "Yes".into())]);
        } else {
            return;
        }
    }
    panic!("Keycloak's device pages did not settle");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_device_flow_signs_in_through_keycloak() {
    let Ok(issuer) = std::env::var("RILL_TEST_KEYCLOAK") else {
        eprintln!("RILL_TEST_KEYCLOAK not set; skipping");
        return;
    };
    let flow = DeviceFlow {
        client_id: "rill-cli".into(),
        scopes: vec!["openid".into(), "profile".into(), "email".into()],
        device_authorization_endpoint: format!("{issuer}/protocol/openid-connect/auth/device"),
        token_endpoint: format!("{issuer}/protocol/openid-connect/token"),
    };
    let http = reqwest::Client::new();
    let authorization = flow.start(&http).await.unwrap();
    let complete = authorization.verification_uri_complete.clone().unwrap();
    let waiting = {
        let (flow, http, authorization) = (flow.clone(), http.clone(), authorization.clone());
        tokio::spawn(async move { flow.wait(&http, &authorization).await })
    };
    approve(&complete, "alice").await;
    let id_token = tokio::time::timeout(std::time::Duration::from_secs(60), waiting)
        .await
        .expect("approved in time")
        .unwrap()
        .unwrap();
    let payload = id_token.split('.').nth(1).unwrap();
    let claims: serde_json::Value = serde_json::from_slice(
        &base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, payload)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(claims["preferred_username"], "alice");
    assert_eq!(claims["aud"], "rill-cli");
}
