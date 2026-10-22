//! A decoded RPC: the request message, owned, with the call's context
//! (headers, extensions such as the caller, deadline).

use connectrpc::{RequestContext, ServiceRequest};

/// One call to a handler.
pub struct Call<T> {
    ctx: RequestContext,
    message: T,
}

impl<T> Call<T> {
    /// A call carrying `message`, for internal dispatch and tests.
    pub fn with_message(ctx: RequestContext, message: T) -> Self {
        Call { ctx, message }
    }

    /// The request message.
    pub fn get_ref(&self) -> &T {
        &self.message
    }

    /// The request message, taken.
    pub fn into_inner(self) -> T {
        self.message
    }

    /// Extensions the middleware attached, such as the caller.
    pub fn extensions(&self) -> &http::Extensions {
        self.ctx.extensions()
    }

    /// The request headers.
    pub fn headers(&self) -> &http::HeaderMap {
        self.ctx.headers()
    }

    /// The call's context.
    pub fn context(&self) -> &RequestContext {
        &self.ctx
    }

    /// The same call with its message replaced.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Call<U> {
        Call {
            ctx: self.ctx,
            message: f(self.message),
        }
    }
}

impl<T: buffa::view::HasMessageView> Call<T> {
    /// The call a handler received, with its message copied out of the
    /// request buffer.
    pub fn new(ctx: RequestContext, request: ServiceRequest<'_, T>) -> Self {
        Call {
            ctx,
            message: request.to_owned_message(),
        }
    }
}

/// The next message of a client stream, owned; `None` when it ends.
pub async fn next_message<T: buffa::view::HasMessageView>(
    stream: &mut connectrpc::InboundStream<T>,
) -> Result<Option<T>, connectrpc::ConnectError> {
    use futures::StreamExt;
    match stream.next().await {
        None => Ok(None),
        Some(item) => Ok(Some(item?.to_owned_message())),
    }
}
