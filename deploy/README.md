# Deployment

Everything needed to build and run Rill outside `cargo run`.

| Folder | Contents |
| --- | --- |
| [docker](docker/Dockerfile) | The `rill` image: one binary, every role, FoundationDB client, cloud key services and SAML. Build from the repository root: `docker build -f deploy/docker/Dockerfile -t rill .` |
| [helm](helm/rill) | A Helm chart for Kubernetes: deployment, service, config, disruption budget, ServiceMonitor |
| [compose](compose/README.md) | A staging cluster on one machine: FoundationDB, SeaweedFS (S3), three servers behind nginx, Prometheus and Grafana; soak and chaos scripts |
| [grafana](grafana/README.md) | Dashboards for the server's Prometheus metrics |
| [keycloak](keycloak/README.md) | A Keycloak realm for OpenID Connect and SAML sign-in tests |
| [orbstack](orbstack/workload-check.sh) | A check that a pod's Kubernetes service-account token signs in as an agent, on a local OrbStack cluster |

Configuration keys are documented in
[configuration](../architecture/07-runtime/configuration.md); roles,
listeners and shutdown in [server](../architecture/07-runtime/server.md).
