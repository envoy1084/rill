#!/usr/bin/env bash
# A pod in a local Kubernetes cluster acts as a registered agent: its
# projected service account token is exchanged for an agent token by
# `rill` inside the pod. Checks the whole workload path against a server
# on this host.
#
#   KUBE_CONTEXT=orbstack deploy/orbstack/workload-check.sh
#
# Needs the rill:dev image (docker build -f deploy/docker/Dockerfile -t rill:dev .), a debug or release
# `rill` binary, and a cluster that can reach the host as
# host.orb.internal (OrbStack). Refuses any context but the one named.
set -euo pipefail

CONTEXT="${KUBE_CONTEXT:?set KUBE_CONTEXT (only a local cluster)}"
[ "$CONTEXT" = orbstack ] || { echo "refusing context $CONTEXT" >&2; exit 2; }
k() { kubectl --context "$CONTEXT" "$@"; }
RILL="${RILL:-$(pwd)/target/debug/rill}"
PORT="${PORT:-7491}"
HTTP_PORT=$((PORT + 1))
WORK="$(mktemp -d)"
trap 'kill "${SERVER:-0}" 2>/dev/null || true; k -n rill-test delete pod rill-agent --ignore-not-found >/dev/null; rm -rf "$WORK"' EXIT

cat > "$WORK/rill.toml" <<TOML
listen_grpc = "0.0.0.0:$PORT"
listen_http = "0.0.0.0:$HTTP_PORT"
[storage]
kind = "local"
dir = "$WORK/data"
[identity]
admins = ["root"]
[identity.keys]
backend = "local"
master_key_hex = "$(openssl rand -hex 32)"
TOML
ROOT=$("$RILL" api --config "$WORK/rill.toml" mint-token human:root)
"$RILL" api --config "$WORK/rill.toml" >"$WORK/server.log" 2>&1 &
SERVER=$!
for _ in $(seq 50); do curl -sf "http://127.0.0.1:$HTTP_PORT/readyz" >/dev/null && break; sleep 0.2; done
r() { "$RILL" --endpoint "http://127.0.0.1:$PORT" --token "$ROOT" "$@"; }

# The sponsor arrives through SCIM, as from a provisioning system.
SCIM=$(r --json admin scim create | python3 -c 'import json,sys;print(json.load(sys.stdin)["token"])')
curl -sf -X POST "http://127.0.0.1:$HTTP_PORT/scim/v2/default/Users" \
  -H "authorization: Bearer $SCIM" -H 'content-type: application/scim+json' \
  -d '{"userName":"ann","active":true}' >/dev/null

ISSUER=$(k get --raw /.well-known/openid-configuration | python3 -c 'import json,sys;print(json.load(sys.stdin)["issuer"])')
k get --raw /openid/v1/jwks > "$WORK/jwks.json"
r admin trust add cluster --issuer "$ISSUER" --audience rill --jwks-file "$WORK/jwks.json" >/dev/null
r admin agent register builder --sponsor user:ann --repos '*' \
  --workload "$ISSUER=system:serviceaccount:rill-test:*" >/dev/null

k create namespace rill-test --dry-run=client -o yaml | k apply -f - >/dev/null
k -n rill-test create serviceaccount builder --dry-run=client -o yaml | k apply -f - >/dev/null
k -n rill-test delete pod rill-agent --ignore-not-found >/dev/null
k -n rill-test apply -f - >/dev/null <<YAML
apiVersion: v1
kind: Pod
metadata:
  name: rill-agent
spec:
  serviceAccountName: builder
  restartPolicy: Never
  containers:
    - name: rill
      image: rill:dev
      imagePullPolicy: Never
      args: ["--json", "admin", "whoami"]
      env:
        - { name: RILL_ENDPOINT, value: "http://host.orb.internal:$PORT" }
        - { name: RILL_WORKLOAD_TOKEN_FILE, value: /var/run/secrets/rill/token }
        - { name: RILL_MODEL, value: test-model }
      volumeMounts:
        - { name: rill-token, mountPath: /var/run/secrets/rill, readOnly: true }
  volumes:
    - name: rill-token
      projected:
        sources:
          - serviceAccountToken: { audience: rill, expirationSeconds: 600, path: token }
YAML
for _ in $(seq 60); do
  PHASE=$(k -n rill-test get pod rill-agent -o jsonpath='{.status.phase}')
  case "$PHASE" in Succeeded|Failed) break ;; esac
  sleep 2
done
OUT=$(k -n rill-test logs rill-agent)
echo "pod $PHASE: $OUT"
echo "$OUT" | python3 -c '
import json,sys
me=json.load(sys.stdin)
assert me["principal"]["name"]=="builder", me
assert me.get("sponsor")=="user:ann", me
print("the pod acted as agent builder, sponsored by ann")'
