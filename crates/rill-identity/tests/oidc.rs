//! OpenID Connect against the mock provider: the code flow end to end, and
//! every way an ID token must be refused.

use rill_identity::oidc::{authorize_url, exchange_code, AuthorizationRequest, Issuers};
use rill_identity::testing::{MockIdp, MockUser};
use serde_json::json;

async fn idp() -> MockIdp {
    let idp = MockIdp::start().await;
    idp.add_client("rill", Some("s3cret"));
    idp.add_user(MockUser::new("alice").in_groups(&["core"]));
    idp
}

#[tokio::test]
async fn the_code_flow_yields_a_verified_id_token() {
    let idp = idp().await;
    idp.sign_in_as("alice");
    let issuers = Issuers::new(reqwest::Client::new());
    let meta = issuers.metadata(idp.issuer()).await.unwrap();
    let request = AuthorizationRequest::new();
    let redirect = "http://rill.test/auth/callback";
    let url = authorize_url(&meta, "rill", redirect, &["openid".into()], &request).unwrap();

    let no_redirects = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let response = no_redirects.get(&url).send().await.unwrap();
    let location = url::Url::parse(response.headers()["location"].to_str().unwrap()).unwrap();
    let query: std::collections::HashMap<_, _> = location.query_pairs().into_owned().collect();
    assert_eq!(query["state"], request.state);

    // A wrong PKCE verifier cannot redeem the code.
    assert!(exchange_code(
        issuers.client(),
        &meta,
        "rill",
        Some("s3cret"),
        redirect,
        &query["code"],
        "wrong"
    )
    .await
    .is_err());
    // The code was spent by the failed attempt; sign in again.
    let request = AuthorizationRequest::new();
    let url = authorize_url(&meta, "rill", redirect, &["openid".into()], &request).unwrap();
    let response = no_redirects.get(&url).send().await.unwrap();
    let location = url::Url::parse(response.headers()["location"].to_str().unwrap()).unwrap();
    let code = location
        .query_pairs()
        .find(|(k, _)| k == "code")
        .unwrap()
        .1
        .into_owned();
    let tokens = exchange_code(
        issuers.client(),
        &meta,
        "rill",
        Some("s3cret"),
        redirect,
        &code,
        &request.pkce_verifier,
    )
    .await
    .unwrap();

    let verifier = issuers
        .verifier(idp.issuer(), vec!["rill".into()])
        .await
        .unwrap();
    let claims = verifier
        .verify(&tokens.id_token, Some(&request.nonce))
        .await
        .unwrap();
    assert_eq!(claims.str("preferred_username"), Some("alice"));
    assert_eq!(claims.strings("groups"), ["core"]);
    assert!(verifier
        .verify(&tokens.id_token, Some("other"))
        .await
        .is_err());
}

#[tokio::test]
async fn id_tokens_are_refused_unless_everything_checks() {
    let idp = idp().await;
    let issuers = Issuers::new(reqwest::Client::new());
    let verifier = issuers
        .verifier(idp.issuer(), vec!["rill".into()])
        .await
        .unwrap();
    let good = idp.id_token("alice", "rill", None);
    assert!(verifier.verify(&good, None).await.is_ok());

    let other_audience = idp.id_token("alice", "someone-else", None);
    assert!(verifier.verify(&other_audience, None).await.is_err());

    let now = jsonwebtoken::get_current_timestamp();
    let expired = idp.jwt(&json!({
        "iss": idp.issuer(), "sub": "x", "aud": "rill", "exp": now - 3600, "iat": now - 7200,
    }));
    assert!(verifier.verify(&expired, None).await.is_err());
    let wrong_issuer = idp.jwt(&json!({
        "iss": "https://evil", "sub": "x", "aud": "rill", "exp": now + 60,
    }));
    assert!(verifier.verify(&wrong_issuer, None).await.is_err());

    // Symmetric algorithms are refused outright, whatever the key.
    let hs = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        &json!({ "iss": idp.issuer(), "sub": "x", "aud": "rill", "exp": now + 60 }),
        &jsonwebtoken::EncodingKey::from_secret(b"guess"),
    )
    .unwrap();
    assert!(verifier.verify(&hs, None).await.is_err());

    // A token signed by a key the issuer never published, under its kid.
    let impostor = MockIdp::start().await;
    let forged = impostor.jwt(&json!({
        "iss": idp.issuer(), "sub": "x", "aud": "rill", "exp": now + 60,
    }));
    assert!(verifier.verify(&forged, None).await.is_err());
}
