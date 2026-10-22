//! Records each failed call's code for the RPC metrics, which cannot read
//! it from a Connect response (it travels in the body).

use connectrpc::interceptor::{StreamRequest, StreamResponse, UnaryRequest, UnaryResponse};
use connectrpc::{ConnectError, Interceptor, Next, NextStream, PayloadStream, RequestContext};
use rill_runtime::telemetry::RpcCode;

/// The interceptor; registered first so it sees authentication failures.
pub struct RecordCodes;

fn record<T>(ctx: &Option<RpcCode>, result: &Result<T, ConnectError>) {
    if let (Some(slot), Err(e)) = (ctx, result) {
        slot.set(format!("{:?}", e.code));
    }
}

fn slot(ctx: &RequestContext) -> Option<RpcCode> {
    ctx.extensions().get::<RpcCode>().cloned()
}

#[connectrpc::async_trait]
impl Interceptor for RecordCodes {
    async fn intercept_unary(
        &self,
        req: UnaryRequest,
        next: Next<'_>,
    ) -> Result<UnaryResponse, ConnectError> {
        let slot = slot(&req.ctx);
        let result = next.run(req).await;
        record(&slot, &result);
        result
    }

    async fn intercept_streaming(
        &self,
        req: StreamRequest,
        inbound: PayloadStream,
        next: NextStream<'_>,
    ) -> Result<StreamResponse, ConnectError> {
        let slot = slot(&req.ctx);
        let result = next.run(req, inbound).await;
        record(&slot, &result);
        result
    }
}
