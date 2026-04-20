# Kubernetes Cluster Deployment (Talos)

> **Status:** Research cycle completed 2026-04-18. All open gates resolved with existing Spacebot capability; no upstream Rust changes required. `deploy/helm/spacebot/` and `deploy/docker/` aligned with the findings in PRs #70, #71, #72. This document is the tracked decision record — all substantive rationale from the per-gate research notes has been promoted into the sections below.

Research and rationale for deploying Spacebot into the Talos Kubernetes cluster (`ai-k8s/talos-ai-cluster`). Six open questions (G1-G6) surfaced during cluster-side scaffolding; this document captures the answers, the resulting deployment shape, and the verification steps.

## Scope

**In scope.** The Spacebot-side deployment contract for Talos: config path override, metrics gate, bearer-auth-disable path for Envoy SSO, hybrid LLM routing via per-provider `base_url`, `readOnlyRootFilesystem` compatibility, worker sandbox mode, OTLP tracing plumbing, and the Chromium lazy-download behavior of the browser tool.

**Out of scope.** The cluster-side manifest authoring itself (lives in `ai-k8s/talos-ai-cluster`), the `deploy/docker/` Compose stack (`docs/design-docs/docker-compose-variant.md`), the Helm values bundle (`deploy/helm/spacebot/README.md`), and the runtime integration with Spacedrive (`docs/design-docs/spacedrive-integration-pairing.md`).

## Ground truth

| Fact | Source |
|---|---|
| Config path CLI flag | `src/main.rs:19-21` (`-c, --config <PATH>` global flag) |
| Config load path resolution | `src/config/load.rs:430-445` (`load_from_path` sets `instance_dir = path.parent()`) |
| Metrics gate | `src/config/toml_schema.rs:129` (`[metrics].enabled` defaults to `false`) |
| Bearer auth middleware | `src/api/server.rs:346-376` (when `auth_token = None`, returns `next.run(request).await` immediately) |
| Per-provider base URL | `src/config/types.rs:293` (`pub base_url: String` on `ProviderConfig`) |
| Chrome cache path default | `src/config/load.rs:944` (`chrome_cache_dir = instance_dir.join("chrome_cache")`) |
| Chromium lazy download | `src/tools/browser.rs:2344-2365` (`fetch_chrome` via `chromiumoxide::fetcher::BrowserFetcher`) |
| Sandbox modes | `src/sandbox.rs:74-231` (`SandboxMode::{Enabled, Disabled}`) |
| OTLP env-var handling | `src/config/load.rs:975-977` (`otlp_endpoint: std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok()`) |
| OTLP compiled in | `Cargo.toml:47-51` (tracing-opentelemetry unconditional) |
| Metrics gate in Dockerfile | `Dockerfile:34,101` (`--features metrics`) |

## Gate resolutions

### G3 — config path + metrics gate (critical path for v1 ServiceMonitor)

**Question.** Is there a `SPACEBOT_CONFIG_PATH` override or env-var metrics toggle? Without one, mounting a ConfigMap on top of the data PVC requires `subPath`, which breaks Stakater Reloader hot-reload.

**Answer.** The daemon accepts `-c <PATH>` / `--config <PATH>` as a global CLI flag (`src/main.rs:19-21`, `global = true`). The flag is consumed by `cmd_start` at `src/main.rs:386-410`, which calls `Config::load_from_path` at `src/config/load.rs:430-445`. That function sets `instance_dir = path.parent()` and then reads the file from the passed-in path. Data storage still lives under `$SPACEBOT_DIR` (set via env var); the config file can live anywhere.

No env-var shortcut exists for the metrics gate; `[metrics].enabled = true` must be set in the config file. But the CLI flag makes this a ConfigMap-content concern, not a subPath-mount concern.

**Cluster-side impact.** ConfigMap mounts at `/etc/spacebot/` (directory mount, not subPath). Container args pass `-c /etc/spacebot/config.toml`. Stakater Reloader continues to trigger pod rollouts on ConfigMap change.

### G1 — Entra SSO via Envoy SecurityPolicy

**Question.** What happens when `state.auth_token = None`? Can bearer auth be cleanly disabled so Envoy can gate the UI with Entra OIDC?

**Answer.** The middleware at `src/api/server.rs:346-376` short-circuits when `auth_token` is unset:

```rust
let Some(expected_token) = state.auth_token.as_deref() else {
    return next.run(request).await;
};
```

When the ConfigMap omits `[api].auth_token`, every request passes through unauthenticated. Envoy `SecurityPolicy` on the `HTTPRoute` becomes the sole authentication layer. The SPA fallback at `src/api/server.rs:322-323` already serves `index.html` outside the auth middleware, so the Envoy policy covers it correctly.

**Cluster-side impact.** Path 1 (bearer auth fully disabled). Envoy enforces Entra OIDC at the HTTPRoute. No dual-layer auth complexity.

### G2 — hybrid LLM routing (LiteLLM for OpenAI/Anthropic, direct for others)

**Question.** Do `*_BASE_URL` overrides isolate per-provider, or do they leak across OpenAI-compatible providers (Together, Fireworks, Groq)?

**Answer.** Each provider has its own `base_url` field on `ProviderConfig` (`src/config/types.rs:293`). The LLM manager uses model-prefix routing (`anthropic/...` → Anthropic client, `openai/...` → OpenAI client, etc.), not endpoint-discriminated routing. Setting `[llm.provider.openai].base_url` in the config isolates to OpenAI calls only; Together/Fireworks/Groq each have their own `base_url` field and are unaffected.

**Cluster-side impact.** Safe hybrid routing. `[llm.provider.anthropic].base_url` and `[llm.provider.openai].base_url` point at the in-cluster LiteLLM proxy; other providers use their canonical endpoints.

### G5 — `readOnlyRootFilesystem` compatibility

**Question.** What paths does Spacebot write to outside `/data` (the PVC mount)?

**Answer.** Three write paths surfaced:

- **`/tmp`** — bubblewrap temp dirs, libraries expecting a writable `/tmp`, SQLite WAL if configured off-PVC.
- **`/data/chrome_cache`** — Chromium download target. Default resolved via `instance_dir.join("chrome_cache")` at `src/config/load.rs:944`. With `SPACEBOT_DIR=/data`, the default is `/data/chrome_cache`.
- **`$HOME/.config/*`** — avoided by running with `fsGroup: 1000` + all config reads going through the `-c` flag.

**Cluster-side impact.** `readOnlyRootFilesystem: true` with emptyDir mounts for `/tmp` (100 Mi) and `/data/chrome_cache` (500 Mi). The data PVC at `/data` handles the rest.

### G4 — worker sandbox mode inside K8s pod

**Question.** Does `bwrap` preflight work under Talos restricted PodSecurity + `runAsUser: 1000` + `seccompProfile: RuntimeDefault`?

**Answer.** Two modes documented at `src/sandbox.rs:74-231`: `SandboxMode::Enabled` (default) and `SandboxMode::Disabled`. The preflight at `src/sandbox/detection.rs:67-114` runs `bwrap --ro-bind / / --proc /proc -- /bin/true` once at startup. If the detection fails, the daemon logs a warning and falls open (workers run unsandboxed).

Talos ships with `user.max_user_namespaces` enabled by default (from cluster machine-sysctls), and `containerd`'s default seccomp profile (`RuntimeDefault`) allows `clone3` with `CLONE_NEWUSER`. bwrap preflight is expected to succeed; cluster-side verification required.

**Cluster-side impact.** Ship v1.0 with `[sandbox].mode = "disabled"` as a safe default. Post-deploy, verify with:

```bash
kubectl exec -it deploy/spacebot -n ai -- bwrap --ro-bind / / --proc /proc -- /bin/true
```

If exit 0, flip to `enabled` in v1.1.

### G6 — observability feature completeness (OTLP, metrics emission)

**Question.** Are tracing/metrics capabilities feature-gated? Does the production Dockerfile build with all needed features?

**Answer.** Three confirmations:

- **OTLP tracing** compiled in unconditionally (`Cargo.toml:47-51`). Activated by setting `OTEL_EXPORTER_OTLP_ENDPOINT`; `src/config/load.rs:975-977` reads the env var directly. No feature flag.
- **Metrics feature** enabled in the production Dockerfile (`Dockerfile:34,101` has `--features metrics`). All ~35 metrics documented in `METRICS.md` emit when the ConfigMap sets `[metrics].enabled = true`.
- **Four PrometheusRule target metrics** verified to exist: `spacebot_process_errors_total`, `spacebot_context_overflow_total`, `spacebot_worker_duration_seconds`, `spacebot_llm_estimated_cost_dollars`.

No sub-features gate individual metrics.

**Cluster-side impact.** Set `OTEL_EXPORTER_OTLP_ENDPOINT=http://alloy.observability.svc.cluster.local:4317` (Grafana Alloy, OTLP gRPC port) in the deployment spec. ServiceMonitor targets port 9090. PrometheusRules reference the four verified metric names.

### Chromium (side topic)

**Question.** Does the production image ship Chromium? If not, does the browser tool fail gracefully?

**Answer.** Chromium is not bundled. `src/tools/browser.rs:2319-2329` detects system Chrome first via `chromiumoxide::detection::default_executable`; if absent, calls `fetch_chrome(&config.chrome_cache_dir)` to lazy-download (~200 MB, ~30 s first call). `BrowserFetcher::fetch` at `chromiumoxide::fetcher` handles the download.

Failure modes if the pod has no egress to `googleapis.com` / `storage.googleapis.com`: the browser tool returns an error to the agent ("failed to download chrome: ..."). The agent handles the error like any other tool failure; no pod-level crash.

**Cluster-side impact.** Three options, ranked:

1. **Disable the browser tool** in cluster-shipped configurations. Skips the lazy-download entirely. Recommended for v1.0 if no agent actively needs browser automation.
2. **Accept lazy-download.** Confirm pod egress to `googleapis.com`. First browser call eats the ~30 s latency; subsequent calls hit the cached binary.
3. **Fork a Chromium-bundled image.** ~200 MB larger image but zero first-call latency. Deferred to when browser tool becomes load-bearing.

## Deployment shape (resulting cluster manifests)

### ConfigMap (`configmap.yaml.j2`)

```toml
[instance]
dir = "/data"

[api]
host = "0.0.0.0"
port = 19898
# auth_token intentionally omitted — Envoy SecurityPolicy gates via Entra OIDC

[metrics]
enabled = true
port = 9090
bind = "0.0.0.0"

[sandbox]
mode = "disabled"  # v1.0 safe default; flip to "enabled" in v1.1 after bwrap preflight passes

[llm.provider.anthropic]
api_type = "anthropic"
api_key = "${ANTHROPIC_API_KEY}"
base_url = "http://litellm.ai.svc.cluster.local:4000/v1"

[llm.provider.openai]
api_type = "openai_responses"
api_key = "${OPENAI_API_KEY}"
base_url = "http://litellm.ai.svc.cluster.local:4000/v1"

# Other providers (Groq, Fireworks, Together) use their canonical endpoints;
# no base_url override needed.
```

> **Note (2026-04-19):** The production deployment above already premises
> LiteLLM as the model gateway, but the in-tree Spacebot implementation has
> not yet shipped the `[providers.litellm]` configuration surface. The
> LiteLLM Phase 1 plan at `.scratchpad/plans/2026-04-19-litellm-phase-0-and-1.md`
> is the reconciliation work: bringing the in-tree config schema in line with
> the already-deployed posture. No k8s-side changes are needed here; the
> block above is the target operators should use once Phase 1 lands.

### Deployment spec (selected fields; full shape in `deploy/helm/spacebot/values.yaml`)

```yaml
containers:
  - name: spacebot
    args: ["spacebot", "-c", "/etc/spacebot/config.toml", "start", "-f"]
    env:
      - name: SPACEBOT_DIR
        value: "/data"
      - name: OTEL_EXPORTER_OTLP_ENDPOINT
        value: "http://alloy.observability.svc.cluster.local:4317"
      - name: OTEL_SERVICE_NAME
        value: "spacebot"
    envFrom:
      - secretRef:
          name: spacebot-secret  # SOPS-encrypted provider keys
    securityContext:
      readOnlyRootFilesystem: true
      allowPrivilegeEscalation: false
      capabilities:
        drop: ["ALL"]
    volumeMounts:
      - name: data
        mountPath: /data
      - name: config
        mountPath: /etc/spacebot
        readOnly: true
      - name: tmp
        mountPath: /tmp
      - name: chrome-cache
        mountPath: /data/chrome_cache

securityContext:
  runAsNonRoot: true
  runAsUser: 1000
  runAsGroup: 1000
  fsGroup: 1000
  seccompProfile:
    type: RuntimeDefault

volumes:
  - name: data
    persistentVolumeClaim:
      claimName: spacebot-data
  - name: config
    configMap:
      name: spacebot-config
  - name: tmp
    emptyDir:
      sizeLimit: 100Mi
  - name: chrome-cache
    emptyDir:
      sizeLimit: 500Mi
```

**Why `spacebot` is the first arg.** `Dockerfile:149-150` sets `ENTRYPOINT ["docker-entrypoint.sh"]` + `CMD ["spacebot", "start", "--foreground"]`. Kubernetes `args:` fully replaces the Dockerfile CMD; `docker-entrypoint.sh` ends with `exec "$@"`. Without the binary name, the entrypoint tries to exec `-c` as a program. See PR #72 for the regression this caught post-merge.

## Verification

### Pre-deploy

- `cargo test --lib` on the Spacebot side (covers the middleware + config-load tests).
- `helm template deploy/helm/spacebot/values.yaml | kubeconform -strict -summary` to validate the rendered manifest shape.

### Post-deploy

1. **Health probe reaches the daemon:**
   ```bash
   kubectl port-forward svc/spacebot 19898:19898
   curl -v http://localhost:19898/api/health
   ```
   Expect 200 OK, no auth header required.

2. **Metrics endpoint scrapes cleanly:**
   ```bash
   kubectl port-forward svc/spacebot 9090:9090
   curl -s http://localhost:9090/metrics | grep spacebot_
   ```
   Expect all four load-bearing metric families plus the broader telemetry set.

3. **OTLP spans reach Alloy:**
   ```bash
   kubectl logs -n observability deploy/alloy | grep "spacebot"
   ```
   Expect spans annotated with the `OTEL_SERVICE_NAME` value.

4. **bwrap preflight (for v1.1 sandbox enable):**
   ```bash
   kubectl exec -it deploy/spacebot -n ai -- bwrap --ro-bind / / --proc /proc -- /bin/true
   ```
   Exit 0 confirms userns works under Talos PodSecurity. Flip `[sandbox].mode = "enabled"` in the ConfigMap.

5. **Chromium download path (if browser tool is enabled):**
   ```bash
   kubectl exec -it deploy/spacebot -n ai -- ls /data/chrome_cache
   ```
   Empty until first browser invocation. After first call, contains a versioned Chrome binary directory.

## Risk assessment

| Item | Risk | Mitigation |
|---|---|---|
| Bearer auth disabled | Low | Envoy `SecurityPolicy` is the sole auth gate. Unauthenticated pod-internal traffic is blocked by `CiliumNetworkPolicy`. |
| Hybrid LLM routing | Low | Per-provider `base_url` isolation verified in source. No accidental leakage. |
| `readOnlyRootFilesystem` | Low | Specific emptyDir mounts for `/tmp` and Chrome cache. PVC holds durable data. |
| Sandbox disabled (v1.0) | Low | Default is safe. Enable in v1.1 only after bwrap preflight passes. |
| OTLP tracing | Low | Env-var driven; off by default if endpoint unset. Dead endpoints produce retry log spam, not crashes. |
| Chromium lazy-download | Medium | Network dependency on `googleapis.com`. Mitigated by caching on emptyDir. Failure is graceful (tool returns error). |
| First-pod Chrome cache shadow | Low | emptyDir shadows any pre-existing PVC subpath at `/data/chrome_cache` on first start. Cache is regenerable by design. |

## Related documents

- `deploy/helm/spacebot/values.yaml` + `README.md` — values-only wrapper around `bjw-s-labs/app-template` carrying the shape above.
- `deploy/docker/` — Compose variant with the same environment variables and config shape; see `docs/design-docs/docker-compose-variant.md` for the differences that Compose deliberately does not replicate (`readOnlyRootFilesystem`, SOPS, Envoy, etc.).
- `docs/design-docs/k8s-helm-scaffold.md` — earlier decision record for choosing the `bjw-s-labs/app-template` wrapper approach over a published wrapper chart.
- `docs/design-docs/docker-compose-variant.md` — Compose stack rationale, including the "Differences from the Kubernetes deployment" section.
- `docs/design-docs/desktop-sidecar.md` — companion deploy surface (native app, not cluster).
- `METRICS.md` — Prometheus metric catalog; the four PrometheusRule targets live here.
- `src/config/load.rs`, `src/api/server.rs`, `src/tools/browser.rs` — load-bearing source files cited above.

## Future work not in scope here

- **Cluster agent's side of the deployment** — manifest templates, SOPS-encrypted secrets, `HelmRelease`, `CiliumNetworkPolicy`, `PrometheusRule` content. Lives in `ai-k8s/talos-ai-cluster`, not here.
- **Chromium sidecar pattern** — if browser automation becomes load-bearing in the cluster, evaluate the Firecrawl-style Playwright sidecar pattern. Out of scope until demand surfaces.
- **Sandbox enable verification** — the `bwrap` preflight is the v1.1 gate. When verified, a small PR flips the ConfigMap default.
- **Tempo backend for OTLP** — `deploy/docker/` currently exports to Alloy's debug exporter (stdout) for local validation. Cluster ships to Tempo via the observability namespace; a local Tempo container would give dev-loop parity but is deferred.
