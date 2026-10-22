# The image actions run in, and the Bazel client that sends them, so both
# see the same compilers at the same paths.
FROM debian:bookworm
ARG TARGETARCH
ARG BAZELISK_VERSION=v1.25.0
RUN apt-get update \
 && apt-get install -y --no-install-recommends build-essential python3 python-is-python3 \
    git ca-certificates curl unzip zip \
 && rm -rf /var/lib/apt/lists/* \
 && curl -fsSL -o /usr/local/bin/bazel \
    "https://github.com/bazelbuild/bazelisk/releases/download/${BAZELISK_VERSION}/bazelisk-linux-${TARGETARCH}" \
 && chmod +x /usr/local/bin/bazel \
 && useradd --create-home --uid 1000 build
