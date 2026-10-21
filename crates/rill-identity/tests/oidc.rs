//! OpenID Connect against the mock provider: the code flow end to end, and
//! every way an ID token must be refused.

use rill_identity::oidc::{begin, finish, Issuers, Registration};
use rill_identity::testing::{MockIdp, MockUser};
use serde_json::json;

async fn idp() -> MockIdp {
    let idp = MockIdp::start().await;
    idp.add_client("rill", Some("s3cret"));
    idp.add_user(MockUser::new("alice").in_groups(&["core"]));
    idp
}

/// Follow the authorization URL as a signed-in browser would; the code.
async fn code_for(url: &str) -> String {
    let no_redirects = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let response = no_redirects.get(url).send().await.unwrap();
    let location = url::Url::parse(response.headers()["location"].to_str().unwrap()).unwrap();
    location
        .query_pairs()
        .find(|(k, _)| k == "code")
        .unwrap()
        .1
        .into_owned()
}

#[tokio::test]
async fn the_code_flow_yields_verified_claims() {
    let idp = idp().await;
    idp.sign_in_as("alice");
    let registration = Registration {
        issuer: idp.issuer(),
        client_id: "rill",
        client_secret: Some("s3cret"),
        redirect_uri: "http://rill.test/auth/callback",
    };
    let request = begin(&registration, &["openid".into()]).await.unwrap();
    let code = code_for(&request.url).await;
    assert!(
        finish(&registration, &code, "wrong", &request.nonce)
            .await
            .is_err(),
        "a wrong PKCE verifier cannot redeem the code"
    );

    let request = begin(&registration, &["openid".into()]).await.unwrap();
    let code = code_for(&request.url).await;
    assert!(
        finish(&registration, &code, &request.pkce_verifier, "other")
            .await
            .is_err(),
        "the ID token must carry this sign-in's nonce"
    );

    let request = begin(&registration, &["openid".into()]).await.unwrap();
    let code = code_for(&request.url).await;
    let claims = finish(&registration, &code, &request.pkce_verifier, &request.nonce)
        .await
        .unwrap();
    assert_eq!(claims.str("preferred_username"), Some("alice"));
    assert_eq!(claims.strings("groups"), ["core"]);
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
