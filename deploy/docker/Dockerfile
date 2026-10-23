# syntax=docker/dockerfile:1.7
#
# The rill image: one binary, every role (`rill api --roles ...`), with
# FoundationDB client support. Multi-arch (amd64, arm64).
#
#   docker build -f deploy/docker/Dockerfile -t rill .   (from the repository root)
#   docker run rill api --config /etc/rill/rill.toml

# The image's rustup installs the exact toolchain rust-toolchain.toml pins.
ARG RUST_VERSION=1.96.0

FROM rust:${RUST_VERSION}-bookworm AS build
ARG TARGETARCH
# Cargo features of the `rill` binary: FoundationDB, the cloud key services
# and SAML sign-in.
ARG FEATURES=fdb,aws,gcp,saml
ARG FDB_VERSION=7.3.77
ARG FDB_CLIENTS_SHA256_AMD64=b6c263723009bddbc30b6ae8cf9854340bd35cef14152abb0da8fb5af2b91254
ARG FDB_CLIENTS_SHA256_ARM64=b3a5dd972ffd37c1d5dc0c29ffa00e2169bcac62c0186571acced8b006af7cc2
RUN apt-get update \
    && apt-get install -y --no-install-recommends clang libclang-dev pkg-config \
       libxml2-dev libxmlsec1-dev \
    && rm -rf /var/lib/apt/lists/*
RUN set -eu; \
    case "$TARGETARCH" in \
      amd64) arch=amd64; sum=$FDB_CLIENTS_SHA256_AMD64 ;; \
      arm64) arch=aarch64; sum=$FDB_CLIENTS_SHA256_ARM64 ;; \
      *) echo "unsupported architecture $TARGETARCH" >&2; exit 1 ;; \
    esac; \
    deb="foundationdb-clients_${FDB_VERSION}-1_${arch}.deb"; \
    curl -fsSLO "https://github.com/apple/foundationdb/releases/download/${FDB_VERSION}/${deb}"; \
    echo "${sum}  ${deb}" | sha256sum -c; \
    dpkg -i "${deb}"; \
    rm "${deb}"
WORKDIR /src
COPY . .
# The cached target directory outlives the sources, whose copied mtimes
# cargo cannot trust; touching them rebuilds the workspace crates every
# time while dependencies stay cached.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    find crates tools -name '*.rs' -exec touch {} + \
    && cargo build --release --locked -p rill-cli --features "$FEATURES" \
    && cp target/release/rill /usr/local/bin/rill

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates liblzma5 \
       libxml2 libxmlsec1 libxmlsec1-openssl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home rill
COPY --from=build /usr/lib/libfdb_c.so /usr/lib/libfdb_c.so
COPY --from=build /usr/local/bin/rill /usr/local/bin/rill
USER 10001
EXPOSE 7420 7421
ENTRYPOINT ["/usr/local/bin/rill"]
CMD ["api"]
