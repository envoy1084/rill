# Staging on one machine

A small cluster for staging, soak and chaos tests: single-node
FoundationDB, SeaweedFS (S3-compatible) for objects, three rill servers
running every role behind nginx, Prometheus and Grafana with the
dashboards from `../grafana`.

```sh
docker build -t rill:dev ../..
docker compose up -d
export RILL_ENDPOINT=http://localhost:7470 RILL_TOKEN=staging-admin
rill repo create demo
```

| Port | What |
| --- | --- |
| 7470 | gRPC, round-robin over the servers |
| 7471 | object protocol and health |
| 3000 | Grafana |
| 9090 | Prometheus |
| 8333 | S3 (SeaweedFS) |

## Load and chaos

```sh
cargo run --release -p rill-load -- --endpoint http://localhost:7470 \
  --token staging-admin --percent 10 --duration 3600 --watchers 500 \
  --report-every 60
./chaos.sh 3600        # in another shell: kill servers at random
```

`rill-load` reports throughput, latency percentiles against the SLOs, and
its ledger: every version and landing a server acknowledged, read back at
the end. A lost one is a durability bug.

On Apple silicon, FoundationDB runs emulated (its image is amd64 only), so
latencies here say little about production hardware.
