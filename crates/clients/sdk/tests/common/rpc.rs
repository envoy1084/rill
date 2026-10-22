//! Shared plumbing for the mock gRPC services.

use std::future::Future;
use std::pin::Pin;

use tonic::{Response, Status};

pub(super) type RpcResult<T> = Result<Response<T>, Status>;
pub(super) type BoxFut<'a, T> = Pin<Box<dyn Future<Output = RpcResult<T>> + Send + 'a>>;

pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

macro_rules! unimplemented_rpcs {
    ($($name:ident($req:ty) -> $resp:ty;)*) => {
        $(
            fn $name<'a, 'b>(&'a self, _req: Request<$req>) -> BoxFut<'b, $resp>
            where
                'a: 'b,
                Self: 'b,
            {
                Box::pin(async { Err(Status::unimplemented(stringify!($name))) })
            }
        )*
    };
}

pub(super) use unimplemented_rpcs;
