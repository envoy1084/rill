//! Google Cloud KMS has no emulator, so this checks the REST contract
//! against a stand-in server: the request path, bearer token and
//! additional authenticated data, and that what it wraps unwraps.
#![cfg(feature = "gcp")]

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use base64::Engine as _;
use rill_identity::keys::{GcpKms, KeyWrapper};
use serde_json::{json, Value};

const KEY: &str = "projects/p/locations/global/keyRings/r/cryptoKeys/k";

struct FakeAuth;

#[async_trait::async_trait]
impl gcp_auth::TokenProvider for FakeAuth {
    async fn token(&self, _scopes: &[&str]) -> Result<Arc<gcp_auth::Token>, gcp_auth::Error> {
        let token: gcp_auth::Token =
            serde_json::from_value(json!({"access_token": "t0k", "expires_in": 3600})).unwrap();
        Ok(Arc::new(token))
    }

    async fn project_id(&self) -> Result<Arc<str>, gcp_auth::Error> {
        Ok(Arc::from("p"))
    }
}

/// Encrypts by reversing bytes, after checking the request is well-formed.
async fn kms(
    State(()): State<()>,
    Path(rest): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Json<Value> {
    let b64 = base64::engine::general_purpose::STANDARD;
    assert_eq!(headers["authorization"], "Bearer t0k");
    assert_eq!(
        b64.decode(body["additionalAuthenticatedData"].as_str().unwrap())
            .unwrap(),
        b"rill-token-key"
    );
    let (key, op) = rest.rsplit_once(':').unwrap();
    assert_eq!(key, KEY);
    let field = if op == "encrypt" {
        "plaintext"
    } else {
        "ciphertext"
    };
    let mut bytes = b64.decode(body[field].as_str().unwrap()).unwrap();
    bytes.reverse();
    let out = if op == "encrypt" {
        "ciphertext"
    } else {
        "plaintext"
    };
    Json(json!({ out: b64.encode(bytes) }))
}

#[tokio::test]
async fn gcp_kms_speaks_the_rest_contract() {
    let app = Router::new().route("/v1/{*rest}", post(kms)).with_state(());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let service = GcpKms::with_auth(KEY, Some(&format!("http://{addr}")), Arc::new(FakeAuth));
    let wrapped = service.wrap(b"secret").await.unwrap();
    assert_eq!(wrapped.backend, "gcp-kms");
    assert_eq!(wrapped.ciphertext, b"terces");
    assert_eq!(service.unwrap(&wrapped).await.unwrap(), b"secret");
}
