//! Tower middleware that authenticates every gRPC request.
//!
//! Tonic's `Interceptor` is synchronous, and authentication may need the
//! store (team lookups), so this is a plain tower layer around the whole
//! gRPC router. It resolves the caller and stores the [`AuthContext`] in
//! the request extensions, where [`super::caller`] reads it; a failed
//! authentication short-circuits with a gRPC status response.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use http::header::AUTHORIZATION;
use rill_policy::AuthContext;
use tonic::body::Body;
use tonic::Status;
use tower::{Layer, Service};

use super::authenticator::authenticate;
use crate::state::AppState;

/// Layer that adds [`AuthService`] around a service.
#[derive(Clone)]
pub struct AuthLayer {
    state: Arc<AppState>,
}

impl AuthLayer {
    /// A layer authenticating against `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        AuthLayer { state }
    }
}

impl<S> Layer<S> for AuthLayer {
    type Service = AuthService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthService {
            inner,
            state: self.state.clone(),
        }
    }
}

/// The middleware service; see the module docs.
#[derive(Clone)]
pub struct AuthService<S> {
    inner: S,
    state: Arc<AppState>,
}

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

impl<S, B> Service<http::Request<B>> for AuthService<S>
where
    S: Service<http::Request<B>, Response = http::Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
{
    type Response = http::Response<Body>;
    type Error = S::Error;
    type Future = BoxFuture<Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: http::Request<B>) -> Self::Future {
        // Take the ready service and leave a clone behind, so the future
        // owns a service that `poll_ready` already vetted.
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        let state = self.state.clone();
        Box::pin(async move {
            let header = request
                .headers()
                .get(AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            match authenticate(&state, header.as_deref()).await {
                Ok(ctx) => {
                    tracing::Span::current().record("principal", ctx.principal.name.as_str());
                    request.extensions_mut().insert::<AuthContext>(ctx);
                    inner.call(request).await
                }
                Err(e) => {
                    tracing::debug!(error = %e, "request rejected");
                    Ok(Status::from(e).into_http())
                }
            }
        })
    }
}
