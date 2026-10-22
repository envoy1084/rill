//! A span and metrics for every gRPC request.
//!
//! Each request runs in an `rpc` span carrying its request id (the
//! caller's `x-request-id`, or a new one, echoed back in the response) and,
//! once authentication ran, the principal; JSON logs put both on every
//! line. A `traceparent` header makes the span part of the caller's trace.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Instant;

use http::{HeaderMap, HeaderValue};
use opentelemetry::propagation::Extractor;
use tower::{Layer, Service};
use tracing::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use super::metrics::{MethodLabels, Metrics, RpcLabels};

/// Header carrying the request id.
pub const REQUEST_ID: &str = "x-request-id";

/// Layer adding [`TelemetryService`].
#[derive(Clone)]
pub struct TelemetryLayer {
    metrics: Arc<Metrics>,
}

impl TelemetryLayer {
    pub fn new(metrics: Arc<Metrics>) -> Self {
        TelemetryLayer { metrics }
    }
}

impl<S> Layer<S> for TelemetryLayer {
    type Service = TelemetryService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        TelemetryService {
            inner,
            metrics: self.metrics.clone(),
        }
    }
}

/// The middleware service; see the module docs.
#[derive(Clone)]
pub struct TelemetryService<S> {
    inner: S,
    metrics: Arc<Metrics>,
}

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

impl<S, B, R> Service<http::Request<B>> for TelemetryService<S>
where
    S: Service<http::Request<B>, Response = http::Response<R>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
    R: Send + 'static,
{
    type Response = http::Response<R>;
    type Error = S::Error;
    type Future = BoxFuture<Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: http::Request<B>) -> Self::Future {
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        let metrics = self.metrics.clone();
        let (service, method) = split_path(request.uri().path());
        let request_id = request
            .headers()
            .get(REQUEST_ID)
            .and_then(|v| v.to_str().ok())
            .filter(|v| !v.is_empty() && v.len() <= 128)
            .map(str::to_owned)
            .unwrap_or_else(|| hex::encode(rill_types::ids::random_id()));
        let span = tracing::info_span!(
            "rpc",
            rpc = %format!("{service}/{method}"),
            request_id = %request_id,
            principal = tracing::field::Empty,
        );
        let parent = opentelemetry::global::get_text_map_propagator(|p| {
            p.extract(&HeaderExtractor(request.headers()))
        });
        let _ = span.set_parent(parent);
        Box::pin(
            async move {
                let started = Instant::now();
                let outcome = inner.call(request).await;
                let code = match &outcome {
                    Ok(response) => grpc_code(response.headers()),
                    Err(_) => "transport".to_string(),
                };
                metrics
                    .rpc_duration
                    .get_or_create(&MethodLabels {
                        service: service.clone(),
                        method: method.clone(),
                    })
                    .observe(started.elapsed().as_secs_f64());
                metrics
                    .rpc_requests
                    .get_or_create(&RpcLabels {
                        service,
                        method,
                        code,
                    })
                    .inc();
                outcome.map(|mut response| {
                    if let Ok(value) = HeaderValue::from_str(&request_id) {
                        response.headers_mut().insert(REQUEST_ID, value);
                    }
                    response
                })
            }
            .instrument(span),
        )
    }
}

/// `/rill.v1.RepoService/GetRepo` → (`rill.v1.RepoService`, `GetRepo`).
fn split_path(path: &str) -> (String, String) {
    let mut parts = path.trim_start_matches('/').splitn(2, '/');
    let service = parts.next().unwrap_or_default().to_string();
    let method = parts.next().unwrap_or_default().to_string();
    (service, method)
}

/// The status of a response whose status came in its headers (errors are
/// "trailers only"); a status sent in trailers is a success.
fn grpc_code(headers: &HeaderMap) -> String {
    match headers.get("grpc-status").and_then(|v| v.to_str().ok()) {
        Some(code) => format!("{:?}", tonic::Code::from_i32(code.parse().unwrap_or(2))),
        None => format!("{:?}", tonic::Code::Ok),
    }
}

struct HeaderExtractor<'a>(&'a HeaderMap);

impl Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|v| v.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|k| k.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_and_codes() {
        assert_eq!(
            split_path("/rill.v1.RepoService/GetRepo"),
            ("rill.v1.RepoService".to_string(), "GetRepo".to_string())
        );
        let mut headers = HeaderMap::new();
        assert_eq!(grpc_code(&headers), "Ok");
        headers.insert("grpc-status", HeaderValue::from_static("5"));
        assert_eq!(grpc_code(&headers), "NotFound");
    }
}
