# Grafana dashboards

Import into Grafana with a Prometheus data source scraping each server's
`/metrics` (the HTTP listener):

* `rill-overview.json`: RPC rate, errors and latency; metadata store
  (FoundationDB) transactions, conflicts and latency as the servers see
  them.
* `rill-queues.json`: job queues, dead jobs, pending land requests,
  landing and event rates, and lease takeovers, per repository.

Every server reports the same cluster-wide gauges, so the queue panels take
`max` across servers. A test in `rill-api` checks that every metric these
dashboards query exists.
