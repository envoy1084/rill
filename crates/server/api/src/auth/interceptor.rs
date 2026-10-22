//! Authenticates every Connect, gRPC and gRPC-web call to the Rill API.
//!
//! It resolves the caller from the `Authorization` header and stores the
//! [`AuthContext`] in the call's extensions, where [`super::caller`] reads
//! it. A rejected call fails here, encoded for whichever protocol the
//! client spoke. Signing in, health and reflection need no token.

use std::sync::Arc;

use connectrpc::interceptor::{StreamRequest, StreamResponse, UnaryRequest, UnaryResponse};
use connectrpc::{ConnectError, Interceptor, Next, NextStream, PayloadStream, RequestContext};
use http::header::AUTHORIZATION;
use rill_policy::AuthContext;

use super::authenticator::authenticate;
use rill_runtime::state::AppState;

/// Procedures callable without a token, by prefix.
const OPEN: &[&str] = &[
    "/rill.v1.SessionService/",
    "/grpc.health.",
    "/grpc.reflection.",
];

/// The interceptor; see the module docs.
pub struct AuthInterceptor {
    state: Arc<AppState>,
}

impl AuthInterceptor {
    /// An interceptor authenticating against `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        AuthInterceptor { state }
    }

    async fn admit(&self, ctx: &mut RequestContext) -> Result<(), ConnectError> {
        let procedure = ctx.spec().map(|s| s.procedure).unwrap_or_default();
        if OPEN.iter().any(|open| procedure.starts_with(open)) {
            return Ok(());
        }
        let header = ctx
            .header(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let seen = ctx
            .header(super::context::RELATIONS_SEEN_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let state = &self.state;
        let worker = header
            .as_deref()
            .and_then(super::bearer_token)
            .is_some_and(|t| state.worker_secrets.admits(t));
        if worker {
            ctx.extensions_mut().insert(super::ExecWorker);
        }
        let admitted = if worker {
            Ok(super::worker_context())
        } else {
            authenticate(state, header.as_deref())
                .await
                .and_then(|caller| state.admit(&caller).map(|()| caller))
        };
        match admitted {
            Ok(mut caller) => {
                caller.relations_seen = super::context::relations_seen(seen.as_deref());
                tracing::Span::current().record("principal", caller.principal.name.as_str());
                ctx.extensions_mut().insert::<AuthContext>(caller);
                Ok(())
            }
            Err(e) => {
                tracing::debug!(error = %e, procedure, "request rejected");
                Err(e.into())
            }
        }
    }
}

#[connectrpc::async_trait]
impl Interceptor for AuthInterceptor {
    async fn intercept_unary(
        &self,
        mut req: UnaryRequest,
        next: Next<'_>,
    ) -> Result<UnaryResponse, ConnectError> {
        self.admit(&mut req.ctx).await?;
        next.run(req).await
    }

    async fn intercept_streaming(
        &self,
        mut req: StreamRequest,
        inbound: PayloadStream,
        next: NextStream<'_>,
    ) -> Result<StreamResponse, ConnectError> {
        self.admit(&mut req.ctx).await?;
        next.run(req, inbound).await
    }
}
