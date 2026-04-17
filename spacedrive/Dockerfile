# syntax=docker/dockerfile:1.6

# ---- Builder ----
# Uses `rust:trixie` (unpinned like the root Dockerfile). Tracks a current
# stable Rust that satisfies Spacedrive's `channel = "stable"` pin.
# `rust:1.84-trixie` is NOT a published Docker Hub tag; don't use it.
FROM rust:trixie AS builder
SHELL ["/bin/bash", "-o", "pipefail", "-c"]

RUN apt-get update && apt-get install -y --no-install-recommends \
    protobuf-compiler libprotobuf-dev cmake libssl-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy workspace. `.dockerignore` has already trimmed the context.
# NOTE: This naive `COPY . . && cargo build` pattern rebuilds ALL deps on any
# source edit (reviewer sweep Rust-4). The root Spacebot Dockerfile uses a
# stub-build trick (Dockerfile:32-35) to cache deps; replicating that for a
# nested Cargo workspace requires staging every member's Cargo.toml plus any
# build.rs. Deferred — see spec §8 Q3 (cargo-chef). Warm rebuilds under 2
# min once Docker layer cache warms, so the cost is mostly borne on first
# build and after Cargo.lock changes.
COPY . .

# CRITICAL: --no-default-features.
# sd-core's default = ["wasm"] pulls wasmer + wasmer-middlewares. sd-server
# doesn't execute WASM plugins over HTTP. Without this flag, cold build
# balloons to 15-25 min and binary gains ~100 MB for zero functional benefit.
RUN cargo build --release --package sd-server --no-default-features

# Capture dynamic library dependencies for the runtime stage audit.
RUN ldd /build/target/release/sd-server > /build/ldd.txt

# ---- Runtime ----
FROM debian:trixie-slim

# Packages pinned after ldd audit of the builder output.
# - ca-certificates, curl: TLS + healthcheck
# - libdbus-1-3, libsecret-1-0: keyring crate's OS keychain backend
# - libssl3: openssl-sys via sd-core deps
# - libsqlite3-0: belt-and-braces even if sqlx bundles libsqlite3
# Feature-gated libs (ffmpeg, libheif1) only needed with --features ai/ffmpeg.
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl \
    libdbus-1-3 libsecret-1-0 \
    libssl3 libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/*

# Non-root runtime user.
RUN useradd --system --uid 10001 --home /data sd \
    && mkdir -p /data \
    && chown sd /data

COPY --from=builder /build/target/release/sd-server /usr/local/bin/sd-server

ENV SD_DATA_DIR=/data
EXPOSE 8080

# /health is behind Basic Auth middleware (main.rs:349-358). Pass credentials.
HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD curl -f -u "$SD_AUTH" http://localhost:8080/health || exit 1

USER sd
ENTRYPOINT ["sd-server"]
CMD ["--port", "8080"]
