# Spacebot Docker Compose

Quick-start dev/test stack for Spacebot. One `docker-compose.yml`, six profiles.

## Prerequisites

- Docker Engine 24+
- Docker Compose v2.20+ (check: `docker compose version`)
- 4 GB RAM minimum; 8 GB recommended for the `spacedrive` profile
- Ports available: 19898 (Spacebot), 9090 (metrics), 8080 (Spacedrive), 80/443 (proxy), 9091 (Prometheus), 3000 (Grafana)

All ports are bound to `127.0.0.1` by default (loopback only). Override via `SPACEBOT_BIND=0.0.0.0` in `.env` if you need LAN exposure.

## First-time setup

```bash
cd deploy/docker
cp .env.example .env
# Edit .env — at minimum set ONE LLM provider key and the SPACEBOT_IMAGE_DIGEST.
just compose-up
```

Visit `http://localhost:19898` once `/api/health` returns 200.

## Profiles

| Profile | Activation | What runs |
|---|---|---|
| `default` | `just compose-up` | Spacebot only (published image) |
| `build` | `just compose-up-build` | Spacebot rebuilt from local source (mutually exclusive with `default`) |
| `spacedrive` | `just compose-up-spacedrive` | Spacebot + local Spacedrive for integration testing |
| `proxy` | Layered with any profile | Caddy with local TLS at `spacebot.localhost` |
| `observability` | `just compose-up-observability` | Prometheus + Grafana with pre-wired dashboard + Grafana Alloy (OTLP collector on 4317 gRPC / 4318 HTTP) |
| `tooling` | Layered | dbtools (SQLite inspection) + mcp-stub (test MCP server) |

Full stack: `just compose-up-all`
Stop: `just compose-down`
Wipe volumes: `just compose-reset` (requires typed WIPE confirmation)
Tail logs: `just compose-logs`

## Environment variables

See `.env.example` for the full list. Required:
- At least one LLM provider key (`ANTHROPIC_API_KEY`, etc.)
- `SPACEBOT_IMAGE_DIGEST` (digest-pinned; update per release)
- `SD_AUTH` if the `spacedrive` profile is active (format: `user:password`)
- `GF_ADMIN_USER` + `GF_ADMIN_PASSWORD` if the `observability` profile is active

Optional:
- `OTEL_EXPORTER_OTLP_ENDPOINT` — OTLP collector URL (e.g. `http://alloy:4318` for HTTP, `http://alloy:4317` for gRPC). Defaults to empty (tracing disabled).
- `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` — signal-specific override; takes precedence per OTel spec.
- `OTEL_EXPORTER_OTLP_PROTOCOL` — `http/protobuf` (default), `http/json`, or `grpc`. The `grpc` value requires building the image with `--features otlp-grpc`; without it, `grpc` disables OTLP with a clear error in the logs rather than silently falling back to HTTP.
- `OTEL_EXPORTER_OTLP_HEADERS` — auth headers (e.g. `authorization=Bearer X`). Not propagated to the gRPC exporter.
- `OTEL_SERVICE_NAME` — service name attached to traces. Defaults to `spacebot`.
- `SPACEBOT_DIR` — override the instance directory (SQLite, LanceDB, redb, PID/socket). Wins over `--config` path's parent. Useful for mounting config read-only while keeping data on a writable volume. Empty value is treated as unset.

## Proxy profile: trust the local CA

Caddy generates its own CA on first run. To trust it on the host:

```bash
just compose-proxy-trust
```

To remove (leftover CAs persist after volume wipe):

```bash
just compose-proxy-untrust
```

## Build vs. default profile

`default` and `build` are mutually exclusive. Pick one:
- `just compose-up` → pulls the published image
- `just compose-up-build` → rebuilds from the root `Dockerfile`

Combining them produces a port collision on 19898 (both services would claim the same port).

## Routing through a proxy (LiteLLM)

Override each provider's `base_url` in `config.toml` to route LLM traffic
through a local proxy. Two equivalent TOML forms are accepted.

**Table form (canonical):**

```toml
[llm.providers.anthropic]
api_type = "anthropic"
base_url = "http://litellm:4000/anthropic"
api_key = "env:ANTHROPIC_API_KEY"

[llm.providers.openai]
api_type = "openai_completions"
base_url = "http://litellm:4000/v1"
api_key = "env:OPENAI_API_KEY"
```

**Top-level array form (also accepted; merged into the table form at load time):**

```toml
[[providers]]
name = "anthropic"
api_type = "anthropic"
base_url = "http://litellm:4000/anthropic"
api_key = "env:ANTHROPIC_API_KEY"
```

Valid `api_type` values: `openai_completions`, `openai_chat_completions`,
`kilo_gateway`, `openai_responses`, `anthropic`, `gemini`, `azure`.
**`api_type = "openai"` is invalid.** Use `openai_completions` when proxying
OpenAI.

Env var parity for proxy setups: `ANTHROPIC_BASE_URL`, `OPENAI_API_BASE`
(canonical OpenAI SDK var), and `OPENAI_BASE_URL` (alias) are honored. User
TOML still wins over env.

### `litellm` profile (v0.5.2+)

The Compose stack now ships an optional `litellm` profile that runs
`ghcr.io/berriai/litellm:main-latest` as a sidecar on port 4000. The sample
config at `deploy/docker/litellm_config.yaml` preloads placeholder model
entries for `claude-sonnet-4-6`, `claude-opus-4-7`, and `gpt-5`.

```bash
# .env (gitignored):
#   LITELLM_MASTER_KEY=sk-…   (proxy admin, used to issue virtual keys)
#   ANTHROPIC_API_KEY=…        (upstream passthrough)
#   OPENAI_API_KEY=…           (upstream passthrough)

just compose-up-litellm
```

Note: `LITELLM_MASTER_KEY` must be set in `.env` before starting the profile.
The LiteLLM container refuses to boot without one and will crash-loop. If you
see the `litellm` service restart repeatedly, check `just compose-logs` for
"Master key not set" and populate `.env`.

Then configure Spacebot's `config.toml`:

```toml
[llm.providers.litellm]
api_type = "openai_chat_completions"
base_url = "http://litellm:4000/v1"
api_key = "env:LITELLM_API_KEY"
```

Route specific models via `litellm/<model_name>`: `litellm/claude-sonnet-4-6`,
`litellm/gpt-5`, etc.

**Two different keys:** `LITELLM_MASTER_KEY` is the proxy's admin credential
(used inside LiteLLM to create virtual keys via `/key/generate`).
`LITELLM_API_KEY` is a virtual key issued by the proxy, scoped to a
user/team/budget. That virtual key is what Spacebot uses. Never configure
Spacebot with the master key.

`litellm/`-prefixed model names skip Spacebot's local rate-limit tracking so
the LiteLLM Router owns rate-limit semantics for proxied deployments.

## Security notes

- `.env` is gitignored; do not check it in.
- `docker inspect` and `docker compose config` print environment variables. Anyone with Docker socket access can read them.
- The `dbtools :ro` mount is a footgun guard, not a security boundary. `docker compose exec --user=root dbtools sh` can remount.
- Caddy's admin API (port 2019) is NOT exposed to the host. Only reachable inside the compose network.
- Before exposing Prometheus beyond loopback, audit `METRICS.md` for PII-sensitive labels.

## Troubleshooting

| Symptom | Fix |
|---|---|
| `/api/health` times out | Wait 60s on cold pull; check `just compose-logs` |
| `bun install` failures in spacedrive build | Verify `spacedrive/.dockerignore` exists |
| 401 from Spacedrive health | Confirm `SD_AUTH` is set in `.env` |
| Grafana empty of SpaceBot metrics | `config.toml` must bind-mount at `/etc/spacebot/config.toml` with `[metrics] enabled = true`. The shipped `./config.toml` does this automatically; verify the mount exists via `docker compose exec spacebot ls /etc/spacebot/`. |
| Grafana empty of traces | OTLP is off by default. Set `OTEL_EXPORTER_OTLP_ENDPOINT=http://alloy:4317` in `.env` and `docker compose restart spacebot`. |
| `alloy` logs show no spans | Confirm SpaceBot has `OTEL_EXPORTER_OTLP_ENDPOINT` set; check Alloy's UI at `http://localhost:12345` for pipeline health. |
| SpaceBot log spam "failed to export OTLP traces" after switching profiles | `OTEL_EXPORTER_OTLP_ENDPOINT` is still set in `.env` but Alloy is no longer running. Comment out the line in `.env` and `docker compose restart spacebot`, or re-run with the `observability` profile. |
| First browser-tool call hangs for ~30s | SpaceBot lazy-downloads Chromium (~200 MB) on first `browser_*` tool invocation rather than bundling it. Subsequent calls use the cached binary under `/data/chrome_cache` (K8s) or the `spacebot-data` volume (compose). Expected on first use. |
| Port 19898 already in use | Stop any other spacebot, or change port in `docker-compose.yml` |

## Related

- `../helm/spacebot/` — Kubernetes deployment (production path on Talos)
- `../../docs/content/docs/(getting-started)/docker.mdx` — single-container quick-start (non-compose)
- `../../spacedrive/Dockerfile` — built by the `spacedrive` profile
