# ---- Builder stage ----
# Compiles the React frontend and the Rust binary with the frontend embedded.
FROM rust:trixie AS builder

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# Install build dependencies:
#   protobuf-compiler — ships the `protoc` binary; LanceDB protobuf codegen
#   libprotobuf-dev — provides Google's well-known `.proto` files
#     (e.g. /usr/include/google/protobuf/empty.proto). `lance-encoding`
#     imports google.protobuf.Empty, so protoc needs these on its include
#     path even though nothing in our Rust graph links against libprotobuf
#     at runtime. Removing this package broke the v0.5.0 Docker build.
#   cmake — onig_sys (regex), lz4-sys
#   libssl-dev — openssl-sys (reqwest TLS)
RUN apt-get update && apt-get upgrade -y && apt-get install -y --no-install-recommends \
    protobuf-compiler \
    libprotobuf-dev \
    cmake \
    libssl-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*
RUN curl -fsSL https://bun.sh/install | bash
ENV PATH="/root/.bun/bin:${PATH}"

# Node 24 is required for the OpenCode embed Vite build.
RUN curl -fsSL https://deb.nodesource.com/setup_24.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# 1. Fetch and cache Rust dependencies.
#    cargo fetch needs a valid target, so we create stubs that get replaced later.
#    `--locked` prevents silent Cargo.lock drift between this stub build and the
#    final `cargo build` below. Without it, a lock touch between layers
#    partially invalidates the dep cache.
#    Cargo.toml declares no [lib] target, so we do NOT create an empty src/lib.rs.
#    Auto-discovery would build a phantom lib that gets thrown away on the real
#    build.
COPY Cargo.toml Cargo.lock ./
COPY vendor/ vendor/
RUN mkdir -p src/bin && echo "fn main() {}" > src/main.rs \
    && echo "fn main() {}" > src/bin/openapi_spec.rs \
    && cargo build --release --locked --features metrics,otlp-grpc,embeddings \
    && rm -rf src

# 2. Stage SpaceUI source and build it first.
#    interface/ declares `"workspaces": ["../spaceui/packages/*"]`, so
#    `bun install` in interface/ expects the spaceui packages to exist
#    on disk as symlink targets. Copy them before the interface install,
#    and run the spaceui build so each package emits its `dist/` (tsc
#    types live there).
COPY spaceui/packages/tokens/ spaceui/packages/tokens/
COPY spaceui/packages/primitives/ spaceui/packages/primitives/
COPY spaceui/packages/ai/ spaceui/packages/ai/
COPY spaceui/packages/forms/ spaceui/packages/forms/
COPY spaceui/packages/explorer/ spaceui/packages/explorer/
COPY spaceui/packages/icons/ spaceui/packages/icons/
COPY spaceui/package.json spaceui/bun.lock spaceui/
COPY spaceui/turbo.json spaceui/
COPY spaceui/tsconfig.base.json spaceui/
# Bun 1.3 validates that every workspace member declared in
# spaceui/package.json exists on disk before installing. `.storybook`
# and `examples/*` are dev-only and excluded by .dockerignore, so stub
# their package.json files to satisfy workspace resolution. The turbo
# filter below scopes the actual build to packages/*.
COPY spaceui/.storybook/package.json spaceui/.storybook/
COPY spaceui/examples/showcase/package.json spaceui/examples/showcase/
# hadolint ignore=DL3003
RUN cd spaceui && bun install --frozen-lockfile && bunx turbo run build --filter="./packages/*"

# 3. Install frontend dependencies (resolves @spacedrive/* and @spacebot/*
#    as workspace symlinks into the packages copied above).
#    interface/package.json declares `"workspaces": ["../spaceui/packages/*",
#    "../packages/*"]`, so bun looks for @spacebot/api-client under
#    ../packages/api-client/. It must exist on disk before `bun install`.
#    The api-client package is TypeScript source with no build step (subpath
#    exports point at `.ts` files directly), so a source copy is sufficient.
#    The preinstall hook in interface/package.json invokes
#    scripts/check-workspace-protocol.sh, so copy that script first.
COPY packages/api-client/ packages/api-client/
COPY scripts/check-workspace-protocol.sh scripts/
COPY interface/package.json interface/bun.lock interface/
# hadolint ignore=DL3003
# SKIP_WORKSPACE_PROTOCOL_CHECK=1: the preinstall hook in interface/package.json
# runs scripts/check-workspace-protocol.sh, which requires a git worktree. Docker
# build contexts lack `.git/` (.dockerignore excludes it). The check already ran
# on the CI host (or via `just gate-pr`) before this image build — suppressing it
# here is safe. Never set this in dev shells or CI runner steps.
RUN cd interface && SKIP_WORKSPACE_PROTOCOL_CHECK=1 bun install --frozen-lockfile

# 4. Build the OpenCode embed bundle (live coding UI in Workers tab).
#    Must run before the frontend build so the embed assets in
#    interface/public/opencode-embed/ are included in the Vite output.
COPY scripts/build-opencode-embed.sh scripts/
COPY interface/opencode-embed-src/ interface/opencode-embed-src/
RUN ./scripts/build-opencode-embed.sh

# 5. Build the frontend (includes OpenCode embed assets from step 4).
COPY interface/ interface/
# hadolint ignore=DL3003
RUN cd interface && bun run build

# 6. Copy source and compile the real binary.
#    build.rs is skipped (SPACEBOT_SKIP_FRONTEND_BUILD=1) since the
#    frontend is already built above with the OpenCode embed included.
#    prompts/ is needed for include_str! in src/prompts/text.rs.
#    presets/ is needed for rust-embed in src/factory/presets.rs and
#    include_str! in src/identity/files.rs.
#    skills/ is needed for include_str! in src/skills/builtin.rs.
#    migrations/ is needed for sqlx::migrate! in src/db.rs.
#    docs/ is needed for rust-embed in src/self_awareness.rs.
#    AGENTS.md, README.md, CHANGELOG.md are needed for include_str! in src/self_awareness.rs.
COPY build.rs ./
COPY prompts/ prompts/
COPY presets/ presets/
COPY skills/ skills/
COPY migrations/ migrations/
COPY docs/ docs/
COPY AGENTS.md README.md CHANGELOG.md ./
COPY src/ src/
# The builder stage is discarded after the runtime stage's COPY --from=builder
# pulls only the binary, so no cleanup of /build/target is needed here.
# `--locked` mirrors the stub build at step 1 (see rationale there).
RUN SPACEBOT_SKIP_FRONTEND_BUILD=1 cargo build --release --locked --features metrics,otlp-grpc,embeddings \
    && mv /build/target/release/spacebot /usr/local/bin/spacebot

# ---- Runtime stage ----
# Minimal runtime with Chrome runtime libraries for fetcher-downloaded Chromium.
# Chrome itself is downloaded on first browser tool use and cached on the volume.
FROM debian:trixie-slim

RUN apt-get update && apt-get upgrade -y && apt-get install -y --no-install-recommends \
    ca-certificates \
    libsqlite3-0 \
    curl \
    gh \
    bubblewrap \
    openssh-server \
    # Chrome runtime dependencies — required whether Chrome is system-installed
    # or downloaded by the built-in fetcher. The fetcher provides the browser
    # binary; these are the shared libraries it links against.
    fonts-liberation \
    libnss3 \
    libatk-bridge2.0-0 \
    libdrm2 \
    libxcomposite1 \
    libxdamage1 \
    libxrandr2 \
    libgbm1 \
    libasound2 \
    libpango-1.0-0 \
    libcairo2 \
    libcups2 \
    libxkbcommon0 \
    libxss1 \
    libxtst6 \
    libxfixes3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/local/bin/spacebot /usr/local/bin/spacebot
# Ship migrations alongside the binary. PR 11.1 (v0.6.0) switched src/db.rs
# from compile-time sqlx::migrate!(literal) to runtime
# sqlx::migrate::Migrator::new(Path) so the daemon can dispatch the
# migration tree (per-agent vs instance) and backend (SQLite vs Postgres)
# at startup. The trade-off: migrations must ship on disk next to the
# binary. The WORKDIR below pins relative-path resolution so
# `tree.sqlite_path() = "migrations/global"` resolves against /app.
COPY --from=builder /build/migrations/ /app/migrations/
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

ENV SPACEBOT_DIR=/data
ENV SPACEBOT_DEPLOYMENT=docker
# Anchor relative paths (migrations/global, migrations/) to /app at runtime.
# The daemon resolves migrations from the WORKDIR; without this, Linux
# defaults to /, which doesn't contain the migrations tree, and startup
# fails with `failed to load migrations from migrations/global`.
WORKDIR /app
EXPOSE 19898 18789 9090

HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD curl -f http://localhost:19898/api/health || exit 1

ENTRYPOINT ["docker-entrypoint.sh"]
CMD ["spacebot", "start", "--foreground"]
