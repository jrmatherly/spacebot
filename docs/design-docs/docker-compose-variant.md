# Docker Compose Variant

> **Status:** Implemented 2026-04-17. This document preserves the architecture rationale. Operational guidance for running the stack lives in `deploy/docker/README.md`.

Research and rationale for `deploy/docker/` — the one-file six-profile compose stack shipped to cover dev, test, integration, proxy, observability, and tooling workflows outside Kubernetes. Promoted from the original `/superpowers:brainstorming` design spec after the implementation landed across twelve commits on 2026-04-17 and the four-reviewer sweep that shaped the final form.

## Scope

**In scope.** The architecture rationale for `deploy/docker/`: why one compose file with six profiles instead of multiple files or overrides; why each profile exists; why certain services default to loopback binding; why `dbtools` is read-only; why the `spacedrive` profile needs a separate `spacedrive/Dockerfile`; why `cargo build --no-default-features` for `sd-server`; why Prometheus + Grafana land under a profile instead of an always-on sidecar; why Caddy's admin API is disabled.

**Out of scope.** How to run the stack (`deploy/docker/README.md`), how to run the Helm chart (`deploy/helm/spacebot/README.md`), the full Spacedrive runtime integration contract (`docs/design-docs/spacedrive-integration-pairing.md`), and any single-container quick-start guidance (`docs/content/docs/(getting-started)/docker.mdx`).

## Ground truth

Facts the design has to respect:

| Fact | Source |
|---|---|
| Port 19898 for HTTP API + web UI | `src/main.rs:1685-1699`, `src/config/types.rs:147` |
| Port 9090 for Prometheus metrics (feature-gated) | `src/telemetry/server.rs`, `src/config/types.rs:169` |
| Port 18789 for inbound webhook receiver (opt-in per `spacebot.toml`) | `src/messaging/webhook.rs`, `src/config/types.rs:189` |
| Data dir `/data` via `SPACEBOT_DIR` | `src/config/load.rs:340` |
| `SPACEBOT_DEPLOYMENT=docker` toggles container-aware behavior | `src/config/toml_schema.rs:117` |
| `/api/health` exempted from bearer auth | `src/api/server.rs:356` |
| Embedded databases (SQLite + LanceDB + redb) forbid multi-replica | `src/config/types.rs:1526-1532` |
| Spacedrive `sd-server` Basic Auth wraps `/health` | `spacedrive/apps/server/src/main.rs:349-358` |
| `sd-core` default features include `wasm` (wasmer + Cranelift, 3-5 min cold build) | `spacedrive/core/Cargo.toml` |
| `keyring` crate needs `libdbus-1-3` and `libsecret-1-0` at runtime | `sd-core` Cargo deps (transitive via `keyring`) |

These facts tightened several early design choices. The wasmer default alone would have made the `spacedrive` profile's cold build ~3-5 minutes slower and ~100 MB larger for features `sd-server` does not expose.

## Design

### One file, six profiles

Docker Compose's profile mechanism selects which services run from a single `docker-compose.yml`. The alternatives considered and rejected:

- **Separate compose files per workflow.** Would have required `-f base.yml -f observability.yml -f proxy.yml` composition at the CLI. Wrong tradeoff: the invocation surface leaks the layering decision into every operator command, and `docker compose config` output diverges from any single source file.
- **Override files (`docker-compose.override.yml`).** Conventional but cumulative: a dev who wants observability but not proxy has to maintain their own override. The mental model scales poorly past two dimensions.
- **Single file, no profiles.** Would run everything by default. Caddy, Prometheus, Grafana, dbtools, and mcp-stub would spin up for a user who just wants `docker compose up` to boot Spacebot.

The six profiles capture the real orthogonal concerns:

| Profile | Services | Activation | Use case |
|---|---|---|---|
| `default` | `spacebot` | `docker compose up` | Dev / test. Pull published image, mount volume, go. |
| `build` | `spacebot-build` | `--profile build` | Cut a local image from the root `Dockerfile` to validate a build before pushing. Mutually exclusive with `default`. |
| `spacedrive` | `spacedrive` | `--profile spacedrive` | Spacebot + locally-built Spacedrive server for Track A / B integration work. |
| `proxy` | `caddy` | `--profile proxy` | TLS at `spacebot.localhost` in front of Spacebot. |
| `observability` | `prometheus`, `grafana` | `--profile observability` | Prometheus scrapes `spacebot:9090`; Grafana with pre-wired dashboard from `METRICS.md`. |
| `tooling` | `dbtools`, `mcp-stub` | `--profile tooling` | SQLite shell into the Spacebot data dir; stub MCP server for wiring tests. |

The `default` / `build` mutual exclusion is a structural constraint: both services would bind `127.0.0.1:19898`. Compose does not enforce this; the `just compose-up-build` recipe is the operator-facing guardrail.

### Why `just` recipes wrap compose

Twelve `just compose-*` recipes cover: `up`, `up-build`, `up-spacedrive`, `up-observability`, `up-all`, `down`, `down-compat`, `reset`, `logs`, `proxy-trust`, `proxy-untrust`, `validate`.

Recipes exist for three reasons:

1. **Profile activation is error-prone.** `docker compose --profile observability --profile proxy up` is easy to mistype, and `up-all` needs five profile flags. Recipes name the common combinations.
2. **The destructive reset needs a typed confirmation.** `compose-reset` wipes named volumes. The recipe prompts for literal `WIPE` input before calling `docker compose down -v`.
3. **CI validates every profile.** `.github/workflows/docker-compose.yml` calls `just compose-validate`, which runs `docker compose config` with each profile flag combination. A broken profile surfaces in CI without anyone having to remember the matrix.

`just compose-down-compat` exists because `docker compose down` stops profile services only when those profiles were activated. On Compose v2.20+ the `--profile '*'` flag stops everything; older versions need `-f docker-compose.yml --profile default --profile build --profile spacedrive --profile proxy --profile observability --profile tooling down`. The compat recipe carries the long form.

### Host-port binding defaults to loopback

Every host-bound service uses `${SPACEBOT_BIND:-127.0.0.1}:<port>:<port>`. The default `127.0.0.1` binding means `docker compose up` on a laptop does not expose Spacebot to the LAN. Operators who want LAN access set `SPACEBOT_BIND=0.0.0.0` in `.env`.

This matters because Docker Compose has historically bound host ports on `0.0.0.0` by default. A contributor running the stack on an untrusted network would, without this indirection, publish an unauthenticated `/api/health` plus the full bearer-authed API surface to every device on the same Wi-Fi.

### `dbtools` is read-only by convention, not by boundary

The `dbtools` service mounts the Spacebot data volume with `:ro`. This is a footgun guard, not a security boundary. Anyone with `docker compose exec` access can still run `docker compose exec dbtools sh` and mount the volume rw from inside the container's user namespace.

The read-only mount exists because `sqlite3 /data/spacebot.db` with a warm write transaction can corrupt the database while Spacebot is running. Read-only mount means fat-fingered `.mode dump` + redirect can at worst waste disk, not mangle the live file. Operators who need write access stop Spacebot first.

### `spacedrive/Dockerfile` exists because the root Dockerfile can't build `sd-server`

The root `Dockerfile` uses `cargo build --release --package spacebot`. It cannot build `sd-server` because:

1. The root `Cargo.toml` carries `[workspace] exclude = ["spacedrive"]`, so `cargo` does not see the vendored workspace.
2. The vendored Spacedrive workspace pins `channel = "stable"`, which may differ from Spacebot's Rust toolchain.
3. `sd-core`'s default features include `wasm`, which transitively pulls `wasmer` + Cranelift. Cold compile adds 3-5 minutes and ~100 MB to the final image for zero functionality the `sd-server` integration harness needs.

`spacedrive/Dockerfile` is a separate multi-stage build that:

- Uses `rust:trixie` as builder (unpinned to track the Rust ecosystem's rolling stable).
- Runs `cargo build --release --no-default-features --bin sd-server` inside the `spacedrive/` workspace.
- Ships a `debian:trixie-slim` runtime with `ca-certificates`, `curl`, `libdbus-1-3`, `libsecret-1-0`, `libssl3`, `libsqlite3-0`. The `libdbus` / `libsecret` dependencies come from the `keyring` crate that `sd-core` uses for OS-level secret storage; the healthcheck needs `curl`.
- Runs as a non-root user (`sd`).
- Health-checks `/health` with Basic Auth: `curl -f -u "$SD_AUTH_USER:$SD_AUTH_PASS" http://localhost:<port>/health`. Spacedrive's `/health` is behind Basic Auth in `spacedrive/apps/server/src/main.rs:349-358` — without credentials the container is marked unhealthy. Asymmetric with Spacebot's `/api/health` exemption; this is intentional.

### Why Prometheus + Grafana are an opt-in profile

Production telemetry lives in the Talos cluster. The observability profile is for local dashboard work: verifying a Grafana panel renders, tuning a Prometheus alert, reproducing a metric cardinality issue.

Making observability opt-in keeps the `default` profile's footprint small (one container, no always-on 400 MB Grafana). The `prometheus.yml` scrape config is a near-mirror of `deploy/helm/spacebot/`'s Prometheus scrape rules, so a dashboard that works under compose is very likely to work under Helm. The cardinality-trimming `metric_relabel_configs` guidance lives in `docs/metrics.md` under "Trimming High-Cardinality LLM Series" and is shared by both deployment paths.

### Caddy's admin API is disabled

The `proxy` profile runs Caddy with `admin off` in the Caddyfile. Caddy's admin API exposes full configuration reload over HTTP on `localhost:2019` by default. That is trivially exploitable if the container is accidentally port-published or reachable via a shared Docker network from a compromised neighbor.

Operators who need live reloads restart the Caddy service (`docker compose restart caddy`) instead.

### `.env` scope is per-service, not global

Services declare their own `env_file: .env` (when they need it) rather than inheriting via a global `--env-file` flag. This prevents a Spacebot `.env` containing `ANTHROPIC_API_KEY=...` from leaking into the Caddy, Prometheus, Grafana, or dbtools environments.

Spacebot and `spacedrive` are the only services that read `.env`. Prometheus scrapes a known endpoint without credentials. Grafana reads its provisioning config. Caddy's `Caddyfile` is static. Scoped `.env` is a small surface-area win that matches the production deployment's principle of per-pod secrets.

## Rationale for splitting from the Helm chart

`deploy/helm/spacebot/` and `deploy/docker/` do not share files. The Helm chart targets Talos + Flux and consumes `bjw-s-labs/app-template`. The compose stack targets a developer laptop and does not use Helm at all. The overlap is intentionally narrow: the Prometheus scrape config and the documented environment variable names are the only shared surfaces.

Reasons not to unify:

- Compose does not know about Kubernetes concepts (namespaces, service accounts, RBAC). Dressing the compose file up to look like Helm-lite would leak those concepts without implementing them.
- Helm's values-layering model (`values.yaml` + `values.local.yaml`) does not translate to Compose. Profiles are a different axis.
- Helm assumes orchestrator-managed restarts, probes, and rolling updates. Compose assumes `restart: unless-stopped` and `docker compose up`. The healthcheck semantics differ enough that shared health probes would lie about one platform or the other.

The one forced convergence is that both paths must work with Spacebot's single-replica embedded-database constraint. Neither deployment path supports multi-replica.

## Implementation summary (2026-04-17)

Shipped in twelve commits on 2026-04-17, with a four-reviewer parallel sweep (DevOps/compose, Rust/Dockerfile, security/observability, technical writing) before the final merge:

- `deploy/docker/docker-compose.yml` — 277 lines, six profiles, comments cite the source-of-truth rules they enforce.
- `deploy/docker/Caddyfile` — 28 lines, admin API disabled, `spacebot.localhost` as the hostname.
- `deploy/docker/prometheus.yml` — 15 lines. The authoritative scrape config; the old `examples/prometheus.yml` was removed post-merge and its operator guidance moved into `docs/metrics.md` (see `CHANGELOG.md` entry for `examples/prometheus.yml` removal).
- `deploy/docker/grafana/provisioning/*` + `grafana/dashboards/spacebot.json` — wires the dashboard from `METRICS.md`.
- `deploy/docker/dbtools.Dockerfile` — `alpine` + `sqlite3` + `curl` + non-root user.
- `deploy/docker/mcp-stub/` — minimal MCP server for wiring tests.
- `deploy/docker/README.md` — the operator-facing how-to.
- `spacedrive/Dockerfile` — `sd-server` build with `--no-default-features`.
- Twelve `just compose-*` recipes in `justfile`.
- `.github/workflows/docker-compose.yml` — validates every profile on push.
- `.gitignore` + `.dockerignore` updates.

Reviewer sweep fixed three CRITICAL drafts before merge:
- The original `rust:1.84-trixie` tag does not exist on Docker Hub.
- `sd-core`'s default features included `wasm`; the first draft did not disable it.
- Caddy's admin API was exposed in the first draft.

## Relationship to other documents

- `deploy/docker/README.md` — how to run. Complements this design doc. Line 3 of the shipped `docker-compose.yml` used to point at the scratchpad spec; that comment was retargeted to this design doc in the same commit that landed this file.
- `deploy/helm/spacebot/README.md` — the Kubernetes path. Prometheus scrape config and env var names are deliberately shared; nothing else is.
- `docs/metrics.md` — Prometheus scrape config and cardinality-trimming guidance. Serves both compose and Helm deployments.
- `docs/design-docs/spacedrive-integration-pairing.md` — Spacebot ↔ Spacedrive runtime contract. The `spacedrive` compose profile is the local harness for exercising that contract.
- `docs/design-docs/k8s-helm-scaffold.md` — the parallel "promote from scratchpad" precedent for the Helm chart decision.
- `docs/content/docs/(getting-started)/docker.mdx` — single-container quick-start; the compose stack extends that pattern with stack-level concerns.

## Differences from the Kubernetes deployment

`deploy/helm/spacebot/` and `deploy/docker/` serve different audiences: Kubernetes targets Talos + production, Compose targets a developer laptop. The hardening surface differs accordingly. Operators comparing the two should expect these deliberate gaps:

- **`readOnlyRootFilesystem` and restricted PodSecurity** are Kubernetes-only. The Helm values set `readOnlyRootFilesystem: true`, drop all capabilities, and apply `seccompProfile: RuntimeDefault`. Compose does not apply equivalent `read_only: true` or `cap_drop` directives. Adding them would break the developer loop: bubblewrap needs a writable `/tmp`, the browser tool writes to a Chromium cache, SQLite creates WAL files next to the database. The Kubernetes equivalent solves this with emptyDir mounts; Compose trusts the host's per-user isolation instead.
- **ConfigMap mount pattern.** Kubernetes mounts the `spacebot-config` ConfigMap at `/etc/spacebot/` (directory mount, not subPath, so Stakater Reloader can trigger pod rollouts on change). Compose bind-mounts `deploy/docker/config.toml` read-only at the same path. Both pass `-c /etc/spacebot/config.toml` to the daemon. The difference is who manages the config source: SOPS-encrypted Git for the cluster, a local file the operator can edit for Compose.
- **Secret management.** Kubernetes consumes a SOPS-encrypted `spacebot-secret` via `envFrom`. Compose uses `.env` (gitignored) scoped per-service (not globally) so a SpaceBot `.env` containing provider API keys does not leak into Caddy, Prometheus, or Grafana environments.
- **Entra SSO / auth.** Kubernetes omits `[api].auth_token` from the ConfigMap; Envoy `SecurityPolicy` on the HTTPRoute does Entra OIDC and passes every request through to the daemon. Compose omits `auth_token` too but has no SSO layer. The operator is trusted by virtue of sitting behind localhost-bound ports. When `SPACEBOT_BIND=0.0.0.0` for LAN exposure, add a bearer token in `config.toml` manually.
- **LLM routing.** Kubernetes routes OpenAI/Anthropic calls through a cluster-internal LiteLLM proxy via per-provider `[llm.provider.*].base_url` overrides in the ConfigMap. Compose uses direct provider endpoints by default; the operator can add similar overrides to `deploy/docker/config.toml` if running a local LiteLLM.
- **OTLP tracing backend.** Kubernetes exports traces to Grafana Alloy at `alloy.observability.svc.cluster.local:4317`. Compose's `observability` profile runs its own local Grafana Alloy at `alloy:4317`, configured to print span summaries to stdout rather than forward to Tempo. Same OTLP gRPC port, same receiver config, different downstream: cluster has Tempo for queryable retention; Compose prints spans for dev-loop verification.
- **Resource sizing.** The Helm values request 512 Mi / limit 2 Gi of memory. Compose does not declare `mem_limit`; the container gets whatever Docker allocates. For heavy workloads on laptop-class compute, set `SPACEBOT_MEM_LIMIT` via a compose override, or switch to the Helm path for a minikube cluster.
- **Rolling updates.** Kubernetes uses `strategy: Recreate` because the embedded databases forbid multi-replica; upgrade path is "scale to 0, scale to 1 with new image." Compose uses `restart: unless-stopped` with no graceful handoff. SpaceBot's `/api/idle` endpoint (200 when no active workers) is a Helm-path hook for pre-upgrade drain; Compose does not use it.
- **Observability surface.** Both ship Prometheus scraping `spacebot:9090`. Kubernetes adds a `ServiceMonitor` custom resource; Compose uses a static `scrape_configs` entry. Both need `[metrics].enabled = true` in `config.toml` — the Helm values make this a ConfigMap content concern, Compose ships `deploy/docker/config.toml` with it pre-set.

## Future work not in scope here

- **Multi-host compose topologies** (e.g. Prometheus on a separate host). Not needed for dev; production goes to Helm.
- **Spacedrive `sd-server` feature gating** for AI/STT pipelines. The `spacedrive` profile today runs `sd-server` with `--no-default-features`. Opt-in features would add image layers and are deferred until a specific integration test demands them.
- **SOPS integration** for secrets at rest. Production uses SOPS via the Flux HelmRelease pattern. Compose assumes `.env` on a developer laptop.
- **Rolling-update semantics** for the `default` profile. Compose can model `restart: unless-stopped` but not "drain-before-restart"; Spacebot's `/api/idle` endpoint is a Helm-path hook, not a compose-path hook.
- **Tempo integration for the `observability` profile.** Alloy currently prints span summaries to its own stdout. Adding a Tempo container to the profile would give local dev queryable trace retention, matching the cluster setup. Deferred until someone actively uses trace queries for local debugging.
