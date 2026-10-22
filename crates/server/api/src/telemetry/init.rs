//! Setting up logs and traces for a server process.

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::Resource;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use crate::config::{LogFormat, TelemetryConfig};
use crate::error::{Error, Result};

/// Keeps the trace exporter alive; dropping it flushes pending spans.
pub struct Telemetry {
    provider: Option<SdkTracerProvider>,
}

impl Drop for Telemetry {
    fn drop(&mut self) {
        if let Some(provider) = self.provider.take() {
            if let Err(e) = provider.shutdown() {
                eprintln!("flushing traces failed: {e}");
            }
        }
    }
}

/// Install the process's subscriber: logs as text or JSON (with each
/// line's request id and principal from its span), filtered by `RUST_LOG`
/// (default `info`), and spans exported over OTLP when an endpoint is
/// configured. `traceparent` headers are understood either way.
pub fn init(config: &TelemetryConfig) -> Result<Telemetry> {
    opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let logs = match config.log_format {
        LogFormat::Text => tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .boxed(),
        LogFormat::Json => tracing_subscriber::fmt::layer()
            .json()
            .with_current_span(true)
            .with_span_list(false)
            .with_writer(std::io::stderr)
            .boxed(),
    };
    let provider = match &config.otlp_endpoint {
        Some(endpoint) => {
            let exporter = opentelemetry_otlp::SpanExporter::builder()
                .with_http()
                .with_endpoint(endpoint.clone())
                .build()
                .map_err(|e| Error::Config(format!("otlp exporter: {e}")))?;
            Some(
                SdkTracerProvider::builder()
                    .with_batch_exporter(exporter)
                    .with_resource(
                        Resource::builder()
                            .with_service_name(config.service_name.clone())
                            .build(),
                    )
                    .build(),
            )
        }
        None => None,
    };
    let traces = provider
        .as_ref()
        .map(|provider| tracing_opentelemetry::layer().with_tracer(provider.tracer("rill")));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(logs)
        .with(traces)
        .try_init();
    Ok(Telemetry { provider })
}
