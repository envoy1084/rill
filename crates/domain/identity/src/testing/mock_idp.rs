//! A mock OpenID Connect provider on a local port: discovery, keys,
//! authorization codes with PKCE, the device flow, and JWTs minted on
//! demand. It signs with ES256 and auto-approves as a chosen user.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Form, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine as _;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use p256::elliptic_curve::sec1::ToEncodedPoint;
use p256::pkcs8::EncodePrivateKey;
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const KID: &str = "mock-key-1";

/// A person the provider knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockUser {
    pub subject: String,
    pub username: String,
    pub email: String,
    pub groups: Vec<String>,
}

impl MockUser {
    pub fn new(username: &str) -> Self {
        MockUser {
            subject: format!("sub-{username}"),
            username: username.into(),
            email: format!("{username}@example.com"),
            groups: Vec::new(),
        }
    }

    pub fn in_groups(mut self, groups: &[&str]) -> Self {
        self.groups = groups.iter().map(|g| g.to_string()).collect();
        self
    }
}

struct Code {
    user: String,
    client_id: String,
    nonce: Option<String>,
    challenge: String,
    redirect_uri: String,
}

struct DeviceGrant {
    client_id: String,
    approved: Option<String>,
}

struct Idp {
    issuer: String,
    key: EncodingKey,
    jwk: Value,
    users: Mutex<HashMap<String, MockUser>>,
    clients: Mutex<HashMap<String, Option<String>>>,
    signed_in: Mutex<Option<String>>,
    /// Who approves every new device grant at once, if anyone.
    auto_approve: Mutex<Option<String>>,
    codes: Mutex<HashMap<String, Code>>,
    devices: Mutex<HashMap<String, DeviceGrant>>,
}

/// A running mock provider.
pub struct MockIdp {
    idp: Arc<Idp>,
}

fn b64url() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
}

fn random() -> String {
    b64url().encode(rand::random::<[u8; 16]>())
}

impl MockIdp {
    /// Start a provider on an ephemeral local port.
    pub async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind a local port");
        let issuer = format!("http://{}", listener.local_addr().expect("bound"));
        let secret = p256::SecretKey::from_slice(&loop {
            let candidate: [u8; 32] = rand::random();
            if p256::SecretKey::from_slice(&candidate).is_ok() {
                break candidate;
            }
        })
        .expect("checked above");
        let der = secret.to_pkcs8_der().expect("encode the key");
        let point = secret.public_key().to_encoded_point(false);
        let jwk = json!({
            "kty": "EC", "crv": "P-256", "alg": "ES256", "use": "sig", "kid": KID,
            "x": b64url().encode(point.x().expect("uncompressed")),
            "y": b64url().encode(point.y().expect("uncompressed")),
        });
        let idp = Arc::new(Idp {
            issuer,
            key: EncodingKey::from_ec_der(der.as_bytes()),
            jwk,
            users: Mutex::default(),
            clients: Mutex::default(),
            signed_in: Mutex::default(),
            auto_approve: Mutex::default(),
            codes: Mutex::default(),
            devices: Mutex::default(),
        });
        let app = Router::new()
            .route("/.well-known/openid-configuration", get(discovery))
            .route("/jwks", get(jwks))
            .route("/authorize", get(authorize))
            .route("/token", post(token))
            .route("/device", post(device))
            .with_state(idp.clone());
        tokio::spawn(async move { axum::serve(listener, app).await });
        MockIdp { idp }
    }

    pub fn issuer(&self) -> &str {
        &self.idp.issuer
    }

    /// Register a client; `None` makes it public (no secret).
    pub fn add_client(&self, id: &str, secret: Option<&str>) {
        self.idp
            .clients
            .lock()
            .insert(id.into(), secret.map(String::from));
    }

    pub fn add_user(&self, user: MockUser) {
        self.idp.users.lock().insert(user.username.clone(), user);
    }

    /// Who `/authorize` signs in as.
    pub fn sign_in_as(&self, username: &str) {
        *self.idp.signed_in.lock() = Some(username.into());
    }

    /// Approve every device grant from now on as `username`, as if the
    /// person did so in a browser.
    pub fn auto_approve_devices(&self, username: &str) {
        *self.idp.auto_approve.lock() = Some(username.into());
    }

    /// An ID token for `username`, issued to `audience`.
    pub fn id_token(&self, username: &str, audience: &str, nonce: Option<&str>) -> String {
        self.idp.id_token(username, audience, nonce)
    }

    /// A JWT with exactly `claims`, signed by the provider's key; for
    /// workload tokens and malformed-token tests.
    pub fn jwt(&self, claims: &Value) -> String {
        self.idp.sign(claims)
    }
}

impl Idp {
    fn sign(&self, claims: &Value) -> String {
        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(KID.into());
        encode(&header, claims, &self.key).expect("sign a token")
    }

    fn id_token(&self, username: &str, audience: &str, nonce: Option<&str>) -> String {
        let users = self.users.lock();
        let user = users
            .get(username)
            .unwrap_or_else(|| panic!("mock IdP has no user {username}"));
        let now = jsonwebtoken::get_current_timestamp();
        let mut claims = json!({
            "iss": self.issuer, "sub": user.subject, "aud": audience,
            "iat": now, "exp": now + 300,
            "preferred_username": user.username, "email": user.email,
            "groups": user.groups,
        });
        if let Some(nonce) = nonce {
            claims["nonce"] = json!(nonce);
        }
        self.sign(&claims)
    }

    fn client_ok(&self, id: &str, secret: Option<&str>) -> bool {
        match self.clients.lock().get(id) {
            Some(Some(expected)) => secret == Some(expected.as_str()),
            Some(None) => true,
            None => false,
        }
    }
}

fn oauth_error(status: StatusCode, error: &str) -> Response {
    (status, Json(json!({ "error": error }))).into_response()
}

async fn discovery(State(idp): State<Arc<Idp>>) -> Json<Value> {
    let i = &idp.issuer;
    Json(json!({
        "issuer": i,
        "authorization_endpoint": format!("{i}/authorize"),
        "token_endpoint": format!("{i}/token"),
        "jwks_uri": format!("{i}/jwks"),
        "device_authorization_endpoint": format!("{i}/device"),
        "response_types_supported": ["code"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["ES256"],
        "token_endpoint_auth_methods_supported": ["client_secret_post", "client_secret_basic"],
        "code_challenge_methods_supported": ["S256"],
    }))
}

async fn jwks(State(idp): State<Arc<Idp>>) -> Json<Value> {
    Json(json!({ "keys": [idp.jwk] }))
}

#[derive(Deserialize)]
struct AuthorizeQuery {
    client_id: String,
    redirect_uri: String,
    state: String,
    nonce: Option<String>,
    code_challenge: String,
    code_challenge_method: String,
}

async fn authorize(State(idp): State<Arc<Idp>>, Query(q): Query<AuthorizeQuery>) -> Response {
    if !idp.clients.lock().contains_key(&q.client_id) || q.code_challenge_method != "S256" {
        return oauth_error(StatusCode::BAD_REQUEST, "invalid_request");
    }
    let Some(user) = idp.signed_in.lock().clone() else {
        return oauth_error(StatusCode::FORBIDDEN, "access_denied");
    };
    let code = random();
    idp.codes.lock().insert(
        code.clone(),
        Code {
            user,
            client_id: q.client_id,
            nonce: q.nonce,
            challenge: q.code_challenge,
            redirect_uri: q.redirect_uri.clone(),
        },
    );
    let mut to = url::Url::parse(&q.redirect_uri).expect("redirect_uri is a URL");
    to.query_pairs_mut()
        .append_pair("code", &code)
        .append_pair("state", &q.state);
    Redirect::to(to.as_str()).into_response()
}

#[derive(Deserialize)]
struct TokenForm {
    grant_type: String,
    #[serde(default)]
    client_id: String,
    client_secret: Option<String>,
    code: Option<String>,
    redirect_uri: Option<String>,
    code_verifier: Option<String>,
    device_code: Option<String>,
}

async fn token(
    State(idp): State<Arc<Idp>>,
    headers: axum::http::HeaderMap,
    Form(mut f): Form<TokenForm>,
) -> Response {
    // Client credentials may come as HTTP Basic (RFC 6749 §2.3.1) or in
    // the form.
    if let Some((id, secret)) = basic_credentials(&headers) {
        f.client_id = id;
        f.client_secret = Some(secret);
    }
    if !idp.client_ok(&f.client_id, f.client_secret.as_deref()) {
        return oauth_error(StatusCode::UNAUTHORIZED, "invalid_client");
    }
    let (user, nonce) = match f.grant_type.as_str() {
        "authorization_code" => {
            let Some(code) = f.code.and_then(|c| idp.codes.lock().remove(&c)) else {
                return oauth_error(StatusCode::BAD_REQUEST, "invalid_grant");
            };
            let challenge = b64url().encode(Sha256::digest(
                f.code_verifier.unwrap_or_default().as_bytes(),
            ));
            if code.client_id != f.client_id
                || Some(&code.redirect_uri) != f.redirect_uri.as_ref()
                || challenge != code.challenge
            {
                return oauth_error(StatusCode::BAD_REQUEST, "invalid_grant");
            }
            (code.user, code.nonce)
        }
        "urn:ietf:params:oauth:grant-type:device_code" => {
            let mut devices = idp.devices.lock();
            let key = f.device_code.unwrap_or_default();
            match devices.get(&key) {
                None => return oauth_error(StatusCode::BAD_REQUEST, "expired_token"),
                Some(grant) if grant.client_id != f.client_id => {
                    return oauth_error(StatusCode::BAD_REQUEST, "invalid_grant")
                }
                Some(DeviceGrant { approved: None, .. }) => {
                    return oauth_error(StatusCode::BAD_REQUEST, "authorization_pending")
                }
                Some(_) => {}
            }
            let grant = devices.remove(&key).expect("present");
            (grant.approved.expect("approved"), None)
        }
        _ => return oauth_error(StatusCode::BAD_REQUEST, "unsupported_grant_type"),
    };
    let id_token = idp.id_token(&user, &f.client_id, nonce.as_deref());
    Json(json!({
        "access_token": random(), "token_type": "Bearer", "expires_in": 300,
        "id_token": id_token,
    }))
    .into_response()
}

#[derive(Deserialize)]
struct DeviceForm {
    client_id: String,
}

async fn device(State(idp): State<Arc<Idp>>, Form(f): Form<DeviceForm>) -> Response {
    if !idp.clients.lock().contains_key(&f.client_id) {
        return oauth_error(StatusCode::UNAUTHORIZED, "invalid_client");
    }
    let device_code = random();
    let user_code = random()[..8].to_uppercase();
    idp.devices.lock().insert(
        device_code.clone(),
        DeviceGrant {
            client_id: f.client_id,
            approved: idp.auto_approve.lock().clone(),
        },
    );
    let uri = format!("{}/activate", idp.issuer);
    Json(json!({
        "device_code": device_code, "user_code": user_code,
        "verification_uri": uri,
        "verification_uri_complete": format!("{uri}?user_code={user_code}"),
        "expires_in": 300, "interval": 1,
    }))
    .into_response()
}

/// `client_id` and `client_secret` from an `Authorization: Basic` header,
/// each form-urlencoded as RFC 6749 requires.
fn basic_credentials(headers: &axum::http::HeaderMap) -> Option<(String, String)> {
    let value = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let encoded = value.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let text = String::from_utf8(decoded).ok()?;
    let (id, secret) = text.split_once(':')?;
    let unescape = |s: &str| {
        url::form_urlencoded::parse(format!("x={s}").as_bytes())
            .next()
            .map(|(_, v)| v.into_owned())
            .unwrap_or_default()
    };
    Some((unescape(id), unescape(secret)))
}
