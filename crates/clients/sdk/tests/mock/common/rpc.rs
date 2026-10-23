//! Shared plumbing for the mock services.

use connectrpc::{ConnectError, RequestContext, ServiceRequest};

pub(super) type RpcResult<T> = connectrpc::ServiceResult<T>;

/// A call to a mock: the request headers and the owned message.
pub(super) struct Request<T> {
    headers: http::HeaderMap,
    message: T,
}

impl<T> Request<T> {
    pub(super) fn with_message(ctx: RequestContext, message: T) -> Self {
        Request {
            headers: ctx.headers().clone(),
            message,
        }
    }

    pub(super) fn metadata(&self) -> &http::HeaderMap {
        &self.headers
    }

    pub(super) fn get_ref(&self) -> &T {
        &self.message
    }

    pub(super) fn into_inner(self) -> T {
        self.message
    }
}

impl<T: buffa::view::HasMessageView> Request<T> {
    pub(super) fn new(ctx: RequestContext, request: ServiceRequest<'_, T>) -> Self {
        Self::with_message(ctx, request.to_owned_message())
    }
}

pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The next message of a client stream, owned.
pub(super) async fn next_message<T: buffa::view::HasMessageView>(
    stream: &mut connectrpc::InboundStream<T>,
) -> Result<Option<T>, ConnectError> {
    use futures::StreamExt;
    match stream.next().await {
        None => Ok(None),
        Some(item) => Ok(Some(item?.to_owned_message())),
    }
}

macro_rules! unimplemented_rpcs {
    ($($name:ident($req:ty) -> $resp:ty;)*) => {
        $(
            async fn $name(
                &self,
                _ctx: connectrpc::RequestContext,
                _req: connectrpc::ServiceRequest<'_, $req>,
            ) -> RpcResult<$resp> {
                Err(connectrpc::ConnectError::unimplemented(stringify!($name)))
            }
        )*
    };
}

pub(super) use unimplemented_rpcs;
