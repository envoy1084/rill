# A Docker engine with gVisor's runsc as a runtime, for running remote
# execution workers locally where the host engine has no gVisor (macOS,
# OrbStack). Run it privileged; workers and runners start inside it.
FROM debian:bookworm
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl gnupg docker.io iptables \
 && curl -fsSL https://gvisor.dev/archive.key | gpg --dearmor -o /usr/share/keyrings/gvisor-archive-keyring.gpg \
 && echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/gvisor-archive-keyring.gpg] https://storage.googleapis.com/gvisor/releases release main" \
    > /etc/apt/sources.list.d/gvisor.list \
 && apt-get update && apt-get install -y --no-install-recommends runsc \
 && rm -rf /var/lib/apt/lists/*
# --host-uds lets the worker outside the sandbox reach the runner's socket.
RUN mkdir -p /etc/docker \
 && printf '{"runtimes":{"runsc":{"path":"/usr/bin/runsc","runtimeArgs":["--platform=systrap","--host-uds=all"]}}}\n' \
    > /etc/docker/daemon.json
COPY start.sh /usr/local/bin/start.sh
CMD ["/usr/local/bin/start.sh"]
