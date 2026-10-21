//! Browser sign-in with the authorization-code flow: `openidconnect`
//! discovers the provider, builds the authorization URL with PKCE and a
//! nonce, redeems the code and validates the ID token (signature, issuer,
//! audience, authorized party, expiry, nonce).

use std::future::Future;
use std::pin::Pin;
use std::sync::OnceLock;

use base64::Engine as _;
use openidconnect::core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata};
use openidconnect::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, HttpRequest, HttpResponse, IssuerUrl,
    Nonce, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, TokenResponse as _,
};

use super::jwt::Claims;
use crate::error::{Error, Result};

/// A provider registration: who the server is to the provider.
#[derive(Debug, Clone)]
pub struct Registration<'a> {
    pub issuer: &'a str,
    pub client_id: &'a str,
    pub client_secret: Option<&'a str>,
    pub redirect_uri: &'a str,
}

/// The secrets of one sign-in: `state` ties the callback to it, `nonce`
/// ties the ID token to it, and the PKCE verifier proves the code was
/// requested by whoever redeems it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRequest {
    pub url: String,
    pub state: String,
    pub nonce: String,
    pub pkce_verifier: String,
}

/// Where to send the browser to sign in, with the secrets to keep.
pub async fn begin(
    registration: &Registration<'_>,
    scopes: &[String],
) -> Result<AuthorizationRequest> {
    let client = client(registration).await?;
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let mut request = client.authorize_url(
        CoreAuthenticationFlow::AuthorizationCode,
        CsrfToken::new_random,
        Nonce::new_random,
    );
    for scope in scopes.iter().filter(|s| s.as_str() != "openid") {
        request = request.add_scope(Scope::new(scope.clone()));
    }
    let (url, state, nonce) = request.set_pkce_challenge(challenge).url();
    Ok(AuthorizationRequest {
        url: url.into(),
        state: state.secret().clone(),
        nonce: nonce.secret().clone(),
        pkce_verifier: verifier.secret().clone(),
    })
}

/// Redeem `code` and return the verified ID token's claims.
pub async fn finish(
    registration: &Registration<'_>,
    code: &str,
    pkce_verifier: &str,
    nonce: &str,
) -> Result<Claims> {
    let client = client(registration).await?;
    let tokens = client
        .exchange_code(AuthorizationCode::new(code.to_string()))
        .map_err(|e| Error::Oidc(format!("token endpoint: {e}")))?
        .set_pkce_verifier(PkceCodeVerifier::new(pkce_verifier.to_string()))
        .request_async(&http())
        .await
        .map_err(|e| Error::Oidc(format!("code exchange: {e}")))?;
    let id_token = tokens
        .id_token()
        .ok_or_else(|| Error::Oidc("the provider returned no ID token".into()))?;
    id_token
        .claims(&client.id_token_verifier(), &Nonce::new(nonce.to_string()))
        .map_err(|e| Error::Oidc(format!("ID token: {e}")))?;
    // Verified above; read every claim, provider-specific ones included.
    let payload = id_token
        .to_string()
        .split('.')
        .nth(1)
        .and_then(|p| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(p)
                .ok()
        })
        .ok_or_else(|| Error::Oidc("malformed ID token".into()))?;
    let claims = serde_json::from_slice(&payload)
        .map_err(|e| Error::Oidc(format!("ID token claims: {e}")))?;
    Ok(Claims(claims))
}

async fn client(
    r: &Registration<'_>,
) -> Result<
    CoreClient<
        openidconnect::EndpointSet,
        openidconnect::EndpointNotSet,
        openidconnect::EndpointNotSet,
        openidconnect::EndpointNotSet,
        openidconnect::EndpointMaybeSet,
        openidconnect::EndpointMaybeSet,
    >,
> {
    let issuer =
        IssuerUrl::new(r.issuer.to_string()).map_err(|e| Error::Oidc(format!("issuer: {e}")))?;
    let metadata = CoreProviderMetadata::discover_async(issuer, &http())
        .await
        .map_err(|e| Error::Oidc(format!("discovery of {}: {e}", r.issuer)))?;
    let redirect = RedirectUrl::new(r.redirect_uri.to_string())
        .map_err(|e| Error::Oidc(format!("redirect URI: {e}")))?;
    Ok(CoreClient::from_provider_metadata(
        metadata,
        ClientId::new(r.client_id.to_string()),
        r.client_secret.map(|s| ClientSecret::new(s.to_string())),
    )
    .set_redirect_uri(redirect))
}

/// A failed request to the provider.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct HttpError(String);

type HttpFuture =
    Pin<Box<dyn Future<Output = std::result::Result<HttpResponse, HttpError>> + Send>>;

/// The HTTP client openidconnect calls the provider with: the workspace's
/// reqwest, never following redirects (an OIDC endpoint that redirects is
/// refused rather than followed).
fn http() -> impl Fn(HttpRequest) -> HttpFuture {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    let client = CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap_or_default()
        })
        .clone();
    move |request: HttpRequest| {
        let client = client.clone();
        Box::pin(async move {
            let request =
                reqwest::Request::try_from(request).map_err(|e| HttpError(e.to_string()))?;
            let response = client
                .execute(request)
                .await
                .map_err(|e| HttpError(e.to_string()))?;
            let mut builder = http::Response::builder().status(response.status());
            for (name, value) in response.headers() {
                builder = builder.header(name, value);
            }
            let body = response
                .bytes()
                .await
                .map_err(|e| HttpError(e.to_string()))?;
            builder
                .body(body.to_vec())
                .map_err(|e| HttpError(e.to_string()))
        })
    }
}
