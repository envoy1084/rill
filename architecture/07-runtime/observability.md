# Observability

Every server process exports Prometheus metrics, writes structured logs
and, when an endpoint is configured, sends OpenTelemetry traces. Each gRPC
request runs in a span that carries its request id and principal, so log
lines, traces and client errors for one request can be matched. Health
endpoints tell load balancers and orchestrators whether a process is alive
and whether it should get traffic.

## Health

| Endpoint | Answers |
| --- | --- |
| `GET /livez` | 200 while the process serves HTTP |
| `GET /readyz` | 503 `draining` once shutdown starts; 503 with the failing components when a store is unreachable; else 200 |
| gRPC `grpc.health.v1.Health` | `SERVING` for every service and the empty name, `NOT_SERVING` once draining starts |

Both HTTP endpoints and `/metrics` are on the HTTP listener and need no
token; a workers-only process (`--roles workers`) serves just these.

## Metrics

`GET /metrics` returns the OpenMetrics text format (`prometheus-client`).
All names start with `rill_`.

| Metric | Labels | Meaning |
| --- | --- | --- |
| `rpc_requests_total` | `service`, `method`, `code` | gRPC requests by outcome (`Ok`, `NotFound`, ...) |
| `rpc_duration_seconds` | `service`, `method` | gRPC latency histogram |
| `meta_transactions_total` | `outcome` | metadata transactions: `committed`, `conflict`, `error` |
| `meta_transaction_duration_seconds` | | metadata transaction latency histogram |
| `jobs_pending`, `jobs_dead`, `jobs_retried` | `repo`, `queue` | job queue counters (`derive`, `index`) |
| `land_requests_pending` | `repo` | unfinished land requests |
| `trunk_head` | `repo` | trunk head sequence; its rate is landings |
| `events_written` | `repo` | events written; its rate is state changes |
| `lease_epoch` | `repo`, `role` | lease epochs; a change is a takeover |

RPC metrics come from `TelemetryLayer`, a tower layer around the whole gRPC
server. Transaction metrics come from `InstrumentedMetaStore`, which wraps
the metadata store when the state is built. The per-repository gauges are
cluster-wide values read from the store every 15 s by `sample_forever` for
every loaded repository; every server reports the same values, so
dashboards take `max` across servers.

`deploy/grafana/` holds two dashboards: API and metadata store, and
queues, landing and leases. A test fails if a dashboard queries a metric no
server exports.

## Traces and logs

`telemetry::init` installs the subscriber for `rill dev` and `rill api`:

* **Logs** go to stderr as text or JSON (`[telemetry] log_format`,
  `RILL_LOG_FORMAT`), filtered by `RUST_LOG` (default `info`). JSON lines
  include the current span's fields.
* **Request span.** `TelemetryLayer` opens an `rpc` span per request with
  `rpc`, `request_id` and an empty `principal` that `AuthLayer` fills after
  authenticating. The request id is the caller's `x-request-id` or a new
  one, and is echoed in the response. The SDK sends a fresh id on every
  request.
* **Traces** are exported over OTLP/HTTP when `[telemetry] otlp_endpoint`
  (or `OTEL_EXPORTER_OTLP_ENDPOINT`) is set, with `service.name` from
  `service_name` (`OTEL_SERVICE_NAME`, default `rill`). A W3C
  `traceparent` header makes the request span a child of the caller's
  span. Spans are flushed when the process exits.

## Limitations

* The SDK sends request ids but not `traceparent`; client spans are not
  linked to server spans yet.
* Jobs do not carry the trace of the request that enqueued them.
* No metrics are exported over OTLP; Prometheus scrapes them.
* Object protocol (HTTP) requests get no request span or RPC metrics.
* There is no profiling endpoint.

## Code

* [telemetry/](../../crates/rill-api/src/telemetry/mod.rs): `init.rs`, `metrics.rs`, `rpc.rs`, `meta.rs`, `sampler.rs`
* [objects/mod.rs](../../crates/rill-api/src/objects/mod.rs) (`/livez`, `/readyz`, `/metrics`), [serve.rs](../../crates/rill-api/src/serve.rs) (gRPC health)
* [deploy/grafana](../../deploy/grafana/README.md)

---
← Previous: [Configuration](configuration.md) · [Index](../index.md) · Next: [Cache tier](cache.md) →
