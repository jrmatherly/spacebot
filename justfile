set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    @just --list

preflight:
    ./scripts/preflight.sh

preflight-ci:
    ./scripts/preflight.sh --ci

fmt-check:
    cargo fmt --all -- --check

check-all:
    cargo check --all-targets

clippy-all:
    cargo clippy --all-targets

# Narrowest useful check — library only, no deps recompile. For inner-loop
# iteration (active editing). Run `just gate-pr-fast` before committing.
check-fast:
    cargo clippy --lib --no-deps

# Rebuild the embedded frontend UI. Needed when iterating on interface/src
# TypeScript and verifying via `cargo run -- start` (build.rs no longer
# watches interface/src/ to avoid invalidating the Rust incremental cache
# on every TS save). Use `cd interface && bun run dev` for live HMR instead.
check-frontend:
    cd interface && bun run build

test-lib:
    cargo test --lib

# Run unit tests via cargo-nextest (process-per-test isolation, parallel scheduling).
# Requires `cargo install cargo-nextest`. Reported 2-3× faster than cargo test on
# suites with multiple test binaries; gains are smaller for single-crate lib tests.
# Use this when iterating on tests locally; gate-pr.sh runs cargo test by default
# (or --nextest / GATE_PR_NEXTEST=1 to opt in per invocation).
test-lib-nextest:
    cargo nextest run --lib

test-integration-compile:
    cargo test --tests --no-run

# Build local SpaceUI packages.
spaceui-build:
    cd spaceui && bun install && bun run build

# Retired. interface/package.json declares `"workspaces": ["../spaceui/packages/*"]`,
# so `bun install` in interface/ symlinks @spacedrive/* to local spaceui source
# without needing `bun link`. Kept as a stub for discoverability.
spaceui-link:
    @echo "spaceui-link is retired. Run 'just spaceui-build' then 'cd interface && bun install'."
    @echo "interface declares spaceui packages as workspaces; symlinks are created by bun install."

spaceui-unlink:
    @echo "spaceui-unlink is retired. The workspace protocol does not need unlinking."

gate-pr: preflight
    ./scripts/gate-pr.sh

# Fast local gate — skips clippy and integration-test compile. Use for tight
# iteration loops; run `just gate-pr` before pushing.
gate-pr-fast: preflight
    ./scripts/gate-pr.sh --fast

# Gate-PR with cargo-nextest replacing cargo test for the unit test step.
# Same gates as `just gate-pr` otherwise. Requires cargo-nextest installed.
gate-pr-nextest: preflight
    ./scripts/gate-pr.sh --nextest

# Full debug-info build for deep debugger sessions (variable/type inspection).
# Normal `cargo build` uses line-tables-only per [profile.dev] in Cargo.toml.
debug-build:
    CARGO_PROFILE_DEV_DEBUG=2 cargo build

# Prune stale cargo artifacts (old toolchains, 30+ days untouched). Requires
# `cargo install cargo-sweep` once. After running, consider a deeper recovery
# via `rm -rf target/debug/incremental` — next build will be slower but disk
# drops substantially.
sweep-target:
    cargo sweep --installed
    cargo sweep --time 30
    @echo "Deeper recovery (slower next build): rm -rf target/debug/incremental"

# Nuclear cleanup — removes all workspace build state. Use after long absences,
# heavy branch-switching, or when reproducing a build issue from scratch.
clean-all:
    cargo clean
    rm -rf interface/dist interface/node_modules
    rm -rf spaceui/node_modules spaceui/packages/*/dist
    rm -rf interface/public/opencode-embed
    rm -rf .fastembed_cache
    @echo "Note: ~/.cargo/git/db/serenity-* clones are separate and survive this."

# Lighter cleanup — frontend only, preserves Rust target/.
clean-frontend:
    rm -rf interface/dist interface/node_modules
    rm -rf spaceui/node_modules spaceui/packages/*/dist

typegen:
    cargo run --bin openapi-spec > /tmp/spacebot-openapi.json
    bunx openapi-typescript /tmp/spacebot-openapi.json -o packages/api-client/src/schema.d.ts

check-typegen:
    cargo run --bin openapi-spec > /tmp/spacebot-openapi-check.json
    bunx openapi-typescript /tmp/spacebot-openapi-check.json -o /tmp/spacebot-schema-check.d.ts
    diff packages/api-client/src/schema.d.ts /tmp/spacebot-schema-check.d.ts

gate-pr-ci: preflight-ci
    ./scripts/gate-pr.sh --ci

gate-pr-ci-fast: preflight-ci
    ./scripts/gate-pr.sh --ci --fast

# Build the OpenCode embed bundle from a pinned upstream commit.
# Clones opencode, copies embed entry points, builds with Vite,
# and outputs to interface/public/opencode-embed/.
build-opencode-embed:
    ./scripts/build-opencode-embed.sh

# Build the spacebot binary and copy it into the Tauri sidecar
# binaries directory with the correct target-triple suffix.
bundle-sidecar:
    ./scripts/bundle-sidecar.sh --release

# Enforce that the Tauri sidecar binary name agrees across every reference
# site and does not collide case-insensitively with the desktop host binary
# (the original APFS bug). Also runs automatically inside `just gate-pr`.
check-sidecar-naming:
    ./scripts/check-sidecar-naming.sh

# Run the desktop app in development mode.
# The desktop package script pre-bundles the sidecar, and Tauri starts Vite.
desktop-dev:
    cd desktop && bun run tauri:dev

# Build the full desktop app (sidecar + frontend + Tauri bundle).
# The desktop package script pre-bundles the sidecar, and Tauri builds the frontend.
desktop-build:
    cd desktop && bun run tauri:build

# Update the frontend node_modules hash in nix/default.nix after updating interface dependencies.
# Usage: Update interface/package.json or interface/bun.lock, then run: just update-frontend-hash
update-frontend-hash:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Building frontend-updater to get new hash..."
    output=$(nix --extra-experimental-features "nix-command flakes" build .#frontend-updater 2>&1 || true)
    new_hash=$(echo "$output" | awk '/got:/ {print $2}' || true)

    if [ -z "$new_hash" ]; then
        echo "Error: Failed to extract hash from build output." >&2
        if echo "$output" | grep -q "experimental Nix feature"; then
            echo "" >&2
            echo "Nix rejected the flake command because experimental features are disabled system-wide." >&2
            echo "This recipe passes --extra-experimental-features inline, so the failure is likely a different" >&2
            echo "nix.conf setting. To normalize the environment once per machine:" >&2
            echo "  mkdir -p ~/.config/nix && echo 'experimental-features = nix-command flakes' >> ~/.config/nix/nix.conf" >&2
        fi
        echo "" >&2
        echo "Full build output:" >&2
        echo "$output" >&2
        exit 1
    fi

    echo "New hash: $new_hash"

    # Check if hash is already up to date
    current_hash=$(grep -E 'hash \?' nix/default.nix | head -1 | sed -E 's/.*hash \? "([^"]+)".*/\1/')
    if [ "$current_hash" = "$new_hash" ]; then
        echo "Hash is already up to date!"
        exit 0
    fi

    # Update the hash in nix/default.nix (POSIX-safe in-place edit)
    tmpfile=$(mktemp)
    sed -E "s|hash \? \"[^\"]+\"|hash ? \"$new_hash\"|" nix/default.nix > "$tmpfile"
    mv "$tmpfile" nix/default.nix
    echo "Updated nix/default.nix with new hash"
    echo ""
    echo "Next steps:"
    echo "  1. Review the changes: git diff nix/default.nix"
    echo "  2. Test the build: nix --extra-experimental-features 'nix-command flakes' build .#frontend"
    echo "  3. Commit the changes: git add nix/default.nix && git commit -m 'update: frontend node_modules hash'"

# Update all Nix flake inputs (flake.lock).
# Use this when you want to update dependencies like nixpkgs, crane, etc.
update-flake:
    nix flake update --extra-experimental-features "nix-command flakes"

# ============================================
# Docker Compose recipes (deploy/docker/)
# ============================================

# Quick-start Spacebot via published image
compose-up:
    docker compose -f deploy/docker/docker-compose.yml --profile default up -d

# Source-rebuild Spacebot (mutually exclusive with default)
compose-up-build:
    docker compose -f deploy/docker/docker-compose.yml --profile build up -d --build

# Spacebot + Spacedrive integration harness
compose-up-spacedrive:
    docker compose -f deploy/docker/docker-compose.yml --profile default --profile spacedrive up -d --build

# Observability stack (layered on default)
compose-up-observability:
    docker compose -f deploy/docker/docker-compose.yml --profile default --profile observability up -d

# Spacebot + LiteLLM proxy sidecar (requires LITELLM_MASTER_KEY in .env)
compose-up-litellm:
    docker compose -f deploy/docker/docker-compose.yml --profile default --profile litellm up -d

# Full stack: default + spacedrive + proxy + observability + tooling
compose-up-all:
    docker compose -f deploy/docker/docker-compose.yml \
        --profile default --profile spacedrive --profile proxy --profile observability --profile tooling \
        up -d --build

# Stop all services across all profiles (requires Compose v2.20+)
compose-down:
    docker compose -f deploy/docker/docker-compose.yml --profile '*' down

# Fallback for Compose < 2.20
compose-down-compat:
    docker compose -f deploy/docker/docker-compose.yml \
        --profile default --profile build --profile spacedrive \
        --profile proxy --profile observability --profile tooling --profile litellm \
        down

# DESTRUCTIVE: stop + wipe all named volumes. Requires typed WIPE confirmation.
compose-reset:
    @printf "This will wipe spacebot-data, spacedrive-data, grafana, prometheus, caddy volumes.\nType 'WIPE' to confirm: " && \
        read CONFIRM && [ "$$CONFIRM" = "WIPE" ] || (echo "Aborted." && exit 1)
    docker compose -f deploy/docker/docker-compose.yml --profile '*' down -v

# Tail logs across all running services
compose-logs:
    docker compose -f deploy/docker/docker-compose.yml --profile '*' logs -f --tail=100

# Install Caddy's local CA into the host trust store
compose-proxy-trust:
    docker compose -f deploy/docker/docker-compose.yml exec caddy caddy trust

# Remove Caddy's local CA from the host trust store
compose-proxy-untrust:
    docker compose -f deploy/docker/docker-compose.yml exec caddy caddy untrust

# Validate compose file parses for every profile (CI mirror)
compose-validate:
    SPACEBOT_IMAGE_DIGEST=sha256:aaaa SD_AUTH=admin:x GF_ADMIN_USER=admin GF_ADMIN_PASSWORD=x \
        docker compose -f deploy/docker/docker-compose.yml --profile default config > /dev/null
    SPACEBOT_IMAGE_DIGEST=sha256:aaaa SD_AUTH=admin:x GF_ADMIN_USER=admin GF_ADMIN_PASSWORD=x \
        docker compose -f deploy/docker/docker-compose.yml --profile build config > /dev/null
    SPACEBOT_IMAGE_DIGEST=sha256:aaaa SD_AUTH=admin:x GF_ADMIN_USER=admin GF_ADMIN_PASSWORD=x \
        docker compose -f deploy/docker/docker-compose.yml --profile spacedrive config > /dev/null
    SPACEBOT_IMAGE_DIGEST=sha256:aaaa SD_AUTH=admin:x GF_ADMIN_USER=admin GF_ADMIN_PASSWORD=x \
        docker compose -f deploy/docker/docker-compose.yml --profile proxy config > /dev/null
    SPACEBOT_IMAGE_DIGEST=sha256:aaaa SD_AUTH=admin:x GF_ADMIN_USER=admin GF_ADMIN_PASSWORD=x \
        docker compose -f deploy/docker/docker-compose.yml --profile observability config > /dev/null
    SPACEBOT_IMAGE_DIGEST=sha256:aaaa SD_AUTH=admin:x GF_ADMIN_USER=admin GF_ADMIN_PASSWORD=x \
        docker compose -f deploy/docker/docker-compose.yml --profile tooling config > /dev/null
    SPACEBOT_IMAGE_DIGEST=sha256:aaaa SD_AUTH=admin:x GF_ADMIN_USER=admin GF_ADMIN_PASSWORD=x LITELLM_MASTER_KEY=sk-dummy \
        docker compose -f deploy/docker/docker-compose.yml --profile litellm config > /dev/null
    @echo "All profile combinations parse cleanly."

# ============================================
# SpaceUI hygiene recipes
# ============================================

# Run the workspace-protocol guard over every package.json in the repo.
spaceui-check-workspace:
    bash scripts/check-workspace-protocol.sh

# Audit vite dedupe list against shared spaceui/interface deps.
spaceui-check-dedupe:
    bash scripts/check-vite-dedupe.sh

# Typecheck + build spaceui/, then typecheck interface/ (which needs spaceui dist).
# Add this to the default gate-pr dependency chain if spaceui regressions become
# common, but the cadence is low today so it stays separate.
spaceui-gate: spaceui-check-workspace spaceui-check-dedupe
    cd spaceui && bun install --frozen-lockfile
    cd spaceui && bun run typecheck
    cd spaceui && bun run build
    cd interface && bun install --frozen-lockfile
    cd interface && bunx tsc --noEmit
    cd interface && bun run build
    @echo "spaceui-gate passed."

# Check that path:line anchors in Spacedrive integration ADRs still resolve.
check-adr-anchors:
    bash scripts/check-adr-anchors.sh

# Rebuild the directed knowledge graph for a path (e.g. docs/design-docs).
# Preserves --directed topology that the built-in `graphify update` loses.
# See scripts/graphify-rebuild.sh and .scratchpad/completed/2026-04-21-graphify-research.md.
# --clean drops graphify-out/cache/ before building (use after .graphifyignore edits).
# --snapshot writes GRAPH_REPORT.md.keep for manual milestone commits.
graphify-rebuild path *flags:
    scripts/graphify-rebuild.sh {{path}} {{flags}}

# Drop all graphify outputs (nuclear — regenerate with `just graphify-rebuild`).
graphify-clean:
    rm -rf graphify-out/

# Query an existing graph. Fails with a helpful message if no graph exists.
graphify-query question:
    @if [ ! -f graphify-out/graph.json ]; then \
        echo "error: no graph found. Run 'just graphify-rebuild docs/design-docs' first." >&2; \
        exit 1; \
    fi
    graphify query "{{question}}"
