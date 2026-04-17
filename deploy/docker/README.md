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
| `observability` | `just compose-up-observability` | Prometheus + Grafana with pre-wired dashboard |
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
| Grafana empty | Wait for Prometheus healthcheck; check `depends_on` order |
| Port 19898 already in use | Stop any other spacebot, or change port in `docker-compose.yml` |

## Related

- `deploy/helm/spacebot/` — Kubernetes deployment (production path on Talos)
- `docs/docker.md` — single-container quick-start (non-compose)
- `spacedrive/Dockerfile` — built by the `spacedrive` profile
