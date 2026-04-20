# Spacebot Helm Values

Kubernetes deployment values for Spacebot. This directory does **not** ship a Helm chart. It ships values for the upstream `bjw-s-labs/app-template` chart, which every app in the target cluster uses.

## Why values-only (not a wrapper chart)

The target Talos cluster (`ai-k8s/talos-ai-cluster`) consumes `app-template` directly in every `HelmRelease` (litellm, langfuse, apollos-portal, cluster-docs, ~15 apps). Publishing a Spacebot-specific wrapper chart would make Spacebot the odd app out and add a chart-publishing pipeline (CI job, OCI push, version sync with the image tag) for no benefit. Instead, the cluster's `HelmRelease` references `app-template` and supplies the values from this file.

See [`docs/design-docs/k8s-helm-scaffold.md`](../../../docs/design-docs/k8s-helm-scaffold.md) for the full decision record.

## Files

| File | Purpose |
|------|---------|
| `values.yaml` | Authoritative Spacebot deployment config (production-shaped). |
| `values.local.yaml` | Single-node overrides (emptyDir instead of PVC, disabled ServiceMonitor, verbose logs). |
| `.helmignore` | Standard Helm packaging exclusions (unused since we don't publish, kept for `helm template`/`helm lint` hygiene). |

## Install (production-shaped cluster)

```bash
helm install spacebot oci://ghcr.io/bjw-s-labs/helm/app-template \
  --version 4.6.2 \
  --values deploy/helm/spacebot/values.yaml \
  --namespace ai --create-namespace
```

The chart assumes the cluster already provides:

- A SOPS-encrypted secret named `spacebot-secret` supplying the LLM provider and messaging adapter API keys Spacebot reads from environment variables. The cluster repo's `secret.sops.yaml.j2` creates it.
- A `ConfigMap` named `spacebot-config` carrying `config.toml` as a data key. Required: the container mounts it read-only at `/etc/spacebot/` and passes `-c /etc/spacebot/config.toml` to the daemon. See [`docs/design-docs/k8s-cluster-deployment.md`](../../../docs/design-docs/k8s-cluster-deployment.md) for the ConfigMap shape and the `[metrics]`, `[api]`, `[llm.provider.*]` sections that matter at cluster scope.
- A `StorageClass` capable of fulfilling a 5 Gi `ReadWriteOnce` PVC.
- Ingress configured externally (the cluster repo provisions a `HTTPRoute` at `httproute.yaml.j2`; this chart does not provision ingress).
- Optional: an Envoy `SecurityPolicy` gating the `HTTPRoute` for Entra SSO. When the ConfigMap omits `[api].auth_token`, the daemon disables its bearer-auth middleware and passes every request through, leaving Envoy as the sole authentication layer. Code path at `src/api/server.rs:351-353`; full rationale in [`docs/design-docs/k8s-cluster-deployment.md`](../../../docs/design-docs/k8s-cluster-deployment.md) under G1.

## Install (local single-node cluster)

```bash
helm install spacebot oci://ghcr.io/bjw-s-labs/helm/app-template \
  --version 4.6.2 \
  --values deploy/helm/spacebot/values.yaml \
  --values deploy/helm/spacebot/values.local.yaml
```

`values.local.yaml` drops the PVC requirement (emptyDir), disables the ServiceMonitor (no Prometheus Operator on most laptops), relaxes the startup probe grace period, and removes the `envFrom: spacebot-secret` reference so the pod boots without pre-provisioning secrets.

To reach the UI:

```bash
kubectl port-forward svc/spacebot 19898:19898
open http://localhost:19898
```

## Validate without installing

```bash
helm template spacebot oci://ghcr.io/bjw-s-labs/helm/app-template \
  --version 4.6.2 \
  --values deploy/helm/spacebot/values.yaml
```

Inspect the rendered manifests. If `kubeconform` is installed:

```bash
helm template ... | kubeconform -strict -summary
```

## What this file pins

**In scope** (lives here):

- Image: `ghcr.io/jrmatherly/spacebot:v0.4.1` (override via cluster `HelmRelease` per release).
- Controller: `Deployment`, `replicas: 1`, `strategy: Recreate`. Embedded databases (SQLite, LanceDB, redb) use file-level locking, so multi-replica isn't supported and `RollingUpdate` would deadlock on the RWO PVC.
- Container args: `["-c", "/etc/spacebot/config.toml", "start", "-f"]`. Foreground mode is required in containers; the `-c` flag decouples the config file from the data PVC so the ConfigMap can mount read-only at its own path, preserving Reloader hot-reload semantics that a `subPath` mount would break.
- Ports: `19898` (HTTP API + UI), `9090` (Prometheus metrics; emitted when the cluster ConfigMap sets `[metrics].enabled = true`).
- Volumes:
  - `/data` — single `persistentVolumeClaim` (5 Gi, `ReadWriteOnce`) holding SQLite + LanceDB + redb.
  - `/etc/spacebot` — ConfigMap mount (read-only) for `config.toml`. Directory mount, not `subPath`.
  - `/tmp` — 100 Mi `emptyDir` for bubblewrap temp dirs and libraries expecting a writable `/tmp`.
  - `/data/chrome_cache` — 500 Mi `emptyDir` for the browser tool's lazy-downloaded Chromium binary.
- Probes: liveness / readiness / startup all hit `/api/health`. Startup gives 150 s grace for SQLite migrations.
- Resources: 250 m / 512 Mi requests, 2000 m / 2 Gi limits. Sized for LanceDB + FastEmbed + Chromium + spawned workers; tune down if the browser tool is disabled and the agent count is small.
- Security: non-root (UID 1000), `fsGroup: 1000`, `readOnlyRootFilesystem: true`, `seccompProfile: RuntimeDefault`, `capabilities: drop: ["ALL"]`, `allowPrivilegeEscalation: false`. Restricted PodSecurity baseline compliance. `RuntimeDefault` is compatible with bubblewrap's `clone3`/`CLONE_NEWUSER` requirement when `[sandbox].mode = enabled` is flipped on (see G4 research; default-disabled in v1.0).
- Stakater Reloader annotations so pods restart when `spacebot-secret` or `spacebot-config` change.
- `ServiceMonitor` for Prometheus scraping at `/metrics`.

**Deliberately out of scope** (cluster-repo's job):

- **Ingress (`HTTPRoute`).** Gateway name, hostname, and domain are cluster-specific. Lives at `templates/config/kubernetes/apps/ai/spacebot/app/httproute.yaml.j2`.
- **Secret / ConfigMap content.** This chart references them by name; SOPS-encrypted content lives in the cluster repo at `secret.sops.yaml.j2` and `configmap.yaml.j2`.
- **`CiliumNetworkPolicy`.** Egress rules for outbound LLM provider calls belong in `ciliumnetworkpolicy.yaml.j2` alongside the other cluster-networking policies.
- **Active `imagePullSecrets`.** The `ghcr-pull-secret` already exists in the `ai` namespace; uncomment the block in `values.yaml` only if the GHCR image becomes private.

## Handoff to the cluster repo

When ready to deploy into the Talos cluster, invoke `/cluster-deploy scaffold` from `ai-k8s/talos-ai-cluster`. The skill emits the Flux app directory skeleton:

```
templates/config/kubernetes/apps/ai/spacebot/app/
├── ocirepository.yaml.j2      # oci://ghcr.io/bjw-s-labs/helm/app-template v4.6.2
├── helmrelease.yaml.j2        # references OCIRepository; spec.values from this file
├── httproute.yaml.j2          # ingress (cluster-specific)
├── secret.sops.yaml.j2        # SOPS-encrypted spacebot-secret
├── configmap.yaml.j2          # spacebot-config
├── ciliumnetworkpolicy.yaml.j2
└── kustomization.yaml
```

The `HelmRelease`'s `spec.values:` block is populated from `deploy/helm/spacebot/values.yaml` in this repo, either copied inline or referenced via a Flux `valuesFrom`/ConfigMap pattern. Cluster additions (ingress, secrets, network policy) live only in the cluster repo.

## When to revisit this approach

The values-only approach is right while Spacebot deploys to a single cluster with this deployment pattern. Switch to a publishable wrapper chart (see [`k8s-helm-scaffold.md`](../../../docs/design-docs/k8s-helm-scaffold.md) Option 1b) only when a second independent operator wants to deploy Spacebot outside this cluster. Until then, the values bundle is the lower-overhead option.

## Volume layout

Spacebot needs a writable directory for SQLite, LanceDB, redb, and the IPC
socket / PID file. The recommended Kubernetes pattern is:

- ConfigMap mounted **read-only** at `/etc/spacebot/` (directory mount, not
  `subPath`, so live updates propagate via Stakater Reloader)
- PersistentVolumeClaim mounted **read-write** at `/data`
- Container args: `["spacebot", "-c", "/etc/spacebot/config.toml", "start", "--foreground"]`
- Env: `SPACEBOT_DIR=/data`

`SPACEBOT_DIR` wins over the `--config` path's parent, so the daemon writes
data to `/data` while reading config from `/etc/spacebot/`. Empty
`SPACEBOT_DIR` is treated as unset.

## Routing through a proxy (LiteLLM)

Spacebot can route LLM traffic through a proxy like LiteLLM by overriding
each provider's `base_url`. Two equivalent TOML forms are accepted.

**Table form (canonical):**

```toml
[llm.providers.anthropic]
api_type = "anthropic"
base_url = "http://litellm.ai.svc.cluster.local:4000/anthropic"
api_key = "env:ANTHROPIC_API_KEY"

[llm.providers.openai]
api_type = "openai_completions"
base_url = "http://litellm.ai.svc.cluster.local:4000/v1"
api_key = "env:OPENAI_API_KEY"
```

**Top-level array form (also accepted; merged into the table form at load time):**

```toml
[[providers]]
name = "anthropic"
api_type = "anthropic"
base_url = "http://litellm.ai.svc.cluster.local:4000/anthropic"
api_key = "env:ANTHROPIC_API_KEY"
```

Valid `api_type` values: `openai_completions`, `openai_chat_completions`,
`kilo_gateway`, `openai_responses`, `anthropic`, `gemini`, `azure`.
**`api_type = "openai"` is invalid.** Use `openai_completions` for proxying
OpenAI through LiteLLM.

If both forms exist for the same provider, the table form wins because it
lives at the more specific TOML path.

Env var parity for common proxy setups:

| Env var | Effect |
|---|---|
| `ANTHROPIC_BASE_URL` | Overrides Anthropic provider `base_url` when populated from `ANTHROPIC_API_KEY` |
| `OPENAI_API_BASE` | Overrides OpenAI provider `base_url` (canonical OpenAI SDK var) |
| `OPENAI_BASE_URL` | Alias for `OPENAI_API_BASE` (lower precedence) |

User TOML still wins over env via the internal `or_insert_with` pattern.

### LiteLLM as a first-class provider (v0.5.2+)

Starting in v0.5.2, Spacebot recognizes `[llm.providers.litellm]` as a
first-class provider block. This pattern avoids the per-upstream-provider
`base_url` override shown above. You route everything through a single
LiteLLM endpoint and use `litellm/<model_name>` identifiers:

```toml
[llm.providers.litellm]
api_type = "openai_chat_completions"       # pairs with base_url that ends in /v1
base_url = "http://litellm.ai.svc.cluster.local:4000/v1"
api_key = "env:LITELLM_API_KEY"
```

Pairing rule: `openai_chat_completions` appends `/chat/completions` to the
`base_url`. Use `openai_completions` instead when `base_url` points at the
host without `/v1` (it prepends `/v1/chat/completions` for you). Either
works; don't mix them or you'll hit a double-`/v1` path.

Then route specific models via task-level config or the agent defaults:
`litellm/claude-sonnet-4-6`, `litellm/gpt-5`, `litellm/claude-opus-4-7`, etc.

Two distinct keys matter for LiteLLM:

| Key | Scope | Where it lives |
|---|---|---|
| `LITELLM_MASTER_KEY` | LiteLLM proxy admin credential — used only inside the proxy to authorize `/key/generate` calls | LiteLLM proxy env vars (never on the Spacebot side) |
| `LITELLM_API_KEY` | Virtual key scoped to a user/team/budget | Spacebot env or secret store; referenced from `[llm.providers.litellm]` |

Operators use `LITELLM_MASTER_KEY` once to issue a `sk-*`-prefixed virtual key
via the proxy's `/key/generate` endpoint, then configure Spacebot to use that
virtual key as `LITELLM_API_KEY`.

`litellm/`-prefixed model names skip Spacebot's local rate-limit tracking
so the LiteLLM Router owns rate-limit semantics and Spacebot doesn't
double-track. The `ProviderStatus.litellm` API field reflects whether
the provider block is configured so the Settings UI can render the
LiteLLM card.

## Observability (OTLP)

Spacebot exports traces via OTLP. Default transport is HTTP/protobuf
(port 4318 convention). To use gRPC (port 4317 convention), the image
must be built with `--features otlp-grpc`.

| Env var | Effect |
|---|---|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | OTLP collector URL (e.g. `http://alloy:4318`) |
| `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` | Signal-specific override (takes precedence per OTel spec) |
| `OTEL_EXPORTER_OTLP_PROTOCOL` | `http/protobuf` (default), `http/json`, or `grpc` |
| `OTEL_EXPORTER_OTLP_HEADERS` | Auth headers (e.g. `authorization=Bearer X`) |
| `OTEL_SERVICE_NAME` | Resource service name (defaults to `spacebot`) |

To **disable** OTLP entirely, leave `OTEL_EXPORTER_OTLP_ENDPOINT` unset AND
leave `[telemetry].otlp_endpoint` unset in config.toml. There is no
`telemetry.enabled` field.

Setting `OTEL_EXPORTER_OTLP_PROTOCOL=grpc` against an image built without
`otlp-grpc` disables OTLP with a clear error in the startup logs rather
than silently falling back to HTTP.

**gRPC limitations** (current; tracked as follow-up issues):

- gRPC over HTTPS is not supported. The `--features otlp-grpc` build
  enables plaintext gRPC over HTTP only. For in-cluster collectors
  (typical Alloy/Tempo deployments using `http://` service URLs), this is
  sufficient. For external collectors requiring TLS, use OTLP/HTTP at
  port 4318 instead.
- `OTEL_EXPORTER_OTLP_HEADERS` are not propagated to the gRPC exporter.
  The HTTP exporter honors them. Workaround: use OTLP/HTTP if you need
  auth headers (e.g., for Honeycomb, Datadog, or other SaaS collectors).

## References

- Decision record: [`docs/design-docs/k8s-helm-scaffold.md`](../../../docs/design-docs/k8s-helm-scaffold.md)
- Upstream chart: [bjw-s-labs/helm-charts](https://github.com/bjw-s-labs/helm-charts), `app-template`
- Cluster repo pattern: `ai-k8s/talos-ai-cluster` → `templates/config/kubernetes/apps/ai/` (litellm, langfuse exemplars)
- Cluster deploy workflow: `/cluster-deploy` skill
