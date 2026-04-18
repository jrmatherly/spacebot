# Kubernetes Helm Chart Scaffold

> **Status:** Implemented 2026-04-16. Option 1a was chosen and the values bundle now lives at `deploy/helm/spacebot/`. This document is preserved as the decision record for why the shape was chosen. Operational guidance for the values bundle lives in `deploy/helm/spacebot/README.md`.

Research and recommendations for scaffolding Spacebot's Kubernetes deployment as a Helm chart living in this repo. Researched on 2026-04-16. The ultimate deployment destination is a Talos cluster consuming `bjw-s-labs/app-template`; the chart-shape decision is captured here.

## Scope

**In scope:** Structure, file layout, and content for a Helm chart inside this repo that can be:
- Consumed by the cluster repo's Flux HelmRelease pattern, and
- Run locally against a minikube/kind cluster for testing.

**Out of scope:**
- Writing the cluster repo's `helmrelease.yaml.j2`, `httproute.yaml.j2`, `secret.sops.yaml.j2` — those belong in the cluster repo, guided by the existing `/cluster-deploy` skill.
- Actual deployment.
- Any decision on the three Spacebot-side integration gaps (icons, upstream attribution, Spacedrive runtime). Those were separate research docs authored in `.scratchpad/` at the time; they remain ungitted scratch.

## Ground truth researched

### Spacebot's runtime profile (`src/`)

| Fact | Source |
|---|---|
| Port 19898 for HTTP API + web UI | `src/main.rs:1685-1699`, `src/config/types.rs:147` |
| Port 9090 for Prometheus metrics (feature-gated) | `src/telemetry/server.rs`, `src/config/types.rs:169` |
| Data directory `/data` via `SPACEBOT_DIR` env var | `src/config/load.rs:340` |
| `SPACEBOT_DEPLOYMENT=docker` toggles container-aware behavior | `src/config/toml_schema.rs:117` |
| Health endpoint `/api/health` and `/health` | `src/api/server.rs:356`, `src/api/system.rs:41` |
| `/api/idle` endpoint gates rolling updates (200 when no active workers) | `src/api/system.rs` (grep `pub(super) async fn idle`) |
| 30+ API-key env vars (LLM providers + messaging adapters) | `src/config/load.rs:365-379` |
| Embedded databases: SQLite (`spacebot.db`), LanceDB (`lancedb/`), redb (`config.redb`) — single PVC needed | `src/config/types.rs:1526-1532` |
| SQLite migrations run at startup | `src/main.rs:148`, `src/db/` |
| macOS Keychain fallback to file-based keystore when unavailable | `src/main.rs:1381-1457`, `src/secrets/keystore.rs` |

### Existing deployment artifacts in this repo

| Artifact | Purpose | Relevance |
|---|---|---|
| `Dockerfile` (multi-stage: rust:trixie builder → debian:trixie-slim) | Production image | Direct input for chart's `image.repository` |
| `.github/workflows/release.yml` | Publishes `ghcr.io/${{ github.repository_owner }}/spacebot:v<version>` | Image source for chart values (fork-aware via `github.repository_owner`) |
| `examples/prometheus.yml` | Prometheus scrape config | Sanity-check for metrics endpoint shape |

### Cluster deployment pattern (from `/Users/jason/dev/ai-k8s/talos-ai-cluster/`)

The cluster's `ai` namespace has ~15 apps. The two closest matches to Spacebot's profile are **litellm** and **langfuse**. Both use the same pattern:

1. An `OCIRepository` (or `HelmRepository`) points at an upstream Helm chart
2. A `HelmRelease` references that chart via `chartRef` and provides all customization as `values:`
3. `bjw-s-labs/app-template` (`oci://ghcr.io/bjw-s-labs/helm/app-template`) is the common chart for apps that don't ship their own

**Critical implication:** the cluster idiom is to **consume `app-template` and provide values**, not to ship per-app Helm templates. This changes what a "Helm chart in this repo" should look like.

### The bjw-s app-template

`app-template` is a generic Kubernetes app chart that covers 90% of single-service apps (Deployment, StatefulSet, Service, Ingress, PVC, ConfigMap, Secret, ServiceMonitor, PodSecurityContext, probes, init containers, sidecars). Configuration happens entirely through `values.yaml` — no custom templates needed for most apps.

litellm's HelmRelease values structure:
```yaml
controllers:
  litellm:
    replicas: 1
    pod:
      terminationGracePeriodSeconds: 90
      annotations:
        secret.reloader.stakater.com/reload: litellm-secret
    containers:
      app:
        image:
          repository: ghcr.io/berriai/litellm
          tag: v1.x
        probes:
          liveness: { ... }
          readiness: { ... }
          startup: { ... }
        envFrom:
          - secretRef:
              name: litellm-secret
service:
  app:
    ports:
      http:
        port: 4000
persistence:
  data:
    existingClaim: litellm-data
```

Spacebot fits this pattern cleanly.

## Four scaffold shapes (pick one)

### Option 1a: Values bundle, direct consumption by cluster (RECOMMENDED)

**Location:** `deploy/helm/spacebot/`
**Contents:**
```
deploy/helm/spacebot/
├── values.yaml             # Authoritative Spacebot values for app-template
├── values.local.yaml       # Local-dev overrides (emptyDir, single-node quirks)
├── README.md               # Usage, values reference, local-test walkthrough
└── .helmignore             # Standard Helm ignore
```

**How the cluster consumes it.** The cluster repo's `OCIRepository` points directly at `app-template` (same as every other app). The cluster repo's `HelmRelease.spec.values:` is populated from this repo's `values.yaml`, either inline-copied or referenced as a separate ConfigMap-backed values file. No wrapper chart is published.

```yaml
# Cluster repo (templates/config/kubernetes/apps/ai/spacebot/app/):
#
#   ocirepository.yaml.j2 → oci://ghcr.io/bjw-s-labs/helm/app-template (not spacebot)
#   helmrelease.yaml.j2:
#     spec:
#       chartRef: { kind: OCIRepository, name: spacebot }  # points at app-template
#       values: { ... copy or reference values.yaml from this repo ... }
```

**Pros:**
- **Matches the cluster's actual pattern exactly** — litellm, apollos-portal, cluster-docs, and every other `ai`-namespace app does it this way
- Zero chart-publishing pipeline, zero version-sync problem
- `values.yaml` is the single source of truth; cluster repo's `HelmRelease` references it
- Works locally: `helm install spacebot oci://ghcr.io/bjw-s-labs/helm/app-template --version 4.6.2 --values deploy/helm/spacebot/values.yaml`

**Cons:**
- Not a self-contained "installable Helm chart" (it's values for someone else's chart)
- If Spacebot ever needs a genuinely unique template app-template can't express, this pattern forces a redesign

### Option 1b: Wrapper Helm chart, publishable as OCI

**Location:** `deploy/helm/spacebot/`
**Contents:**
```
deploy/helm/spacebot/
├── Chart.yaml              # Declares dependency on bjw-s app-template
├── values.yaml             # All Spacebot-specific config
├── README.md
└── .helmignore
```

`Chart.yaml` declares a dependency:
```yaml
apiVersion: v2
name: spacebot
version: 0.4.1
appVersion: "0.4.1"
dependencies:
  - name: app-template
    version: 4.6.2
    repository: oci://ghcr.io/bjw-s-labs/helm
```

Publishable as `oci://ghcr.io/jrmatherly/spacebot-chart:<version>`. The cluster repo's `OCIRepository` points at the wrapper; customization split between wrapper values and `HelmRelease.spec.values`.

**Pros:**
- Genuine self-contained Helm chart, `helm install`-able standalone
- Independent chart versioning (decoupled from image version)
- If another cluster or third party ever deploys Spacebot, they get one chart, one tag

**Cons:**
- **Diverges from the cluster's established pattern** — no other `ai` app publishes a wrapper
- Adds a chart-publishing pipeline (CI job, version tagging, OCI push)
- Creates a version-sync problem: chart version vs. appVersion vs. image tag
- Extra hop in deploys (cluster → wrapper → app-template)

### Option 2: Pure values bundle, single file in `deploy/`

**Location:** `deploy/kubernetes-values.yaml`
**Contents:** just a values file + comments.

Simpler than 1a (no README, no `.helmignore`) but also less discoverable. Essentially 1a collapsed to one file.

### Option 3: Self-contained Helm chart with custom templates

**Location:** `deploy/helm/spacebot/templates/{deployment,service,pvc,ingress,configmap,secret,servicemonitor}.yaml`

**Pros:** No external dependency; full control over every field.
**Cons:** ~300-500 lines of YAML to maintain; diverges from cluster convention; reinvents what app-template already solves. This is the anti-recommendation.

## Recommendation: Option 1a

Reasons ranked:

1. **Matches the cluster's actual pattern, not a theoretical one.** Verified against litellm, apollos-portal, cluster-docs, and langfuse — all use direct `app-template` consumption with inline values. Publishing a wrapper (1b) would make Spacebot the odd app out.
2. **Zero chart-publishing infrastructure needed.** No CI step, no OCI push, no chart version to sync with the image version. One fewer moving part.
3. **Minimizes maintenance.** When Kubernetes API versions shift (e.g., `autoscaling/v2beta1` → `autoscaling/v2`), app-template handles the migration. No Spacebot-side work.
4. **`helm install` still works locally** — just against `app-template` directly with this repo's `values.yaml`.

**When to choose 1b instead.** If a second operator (outside this cluster) ever needs to deploy Spacebot, the wrapper chart becomes worth its cost. Until that happens, 1a is the lower-overhead option that matches the cluster reality.

## Concrete `values.yaml` draft

This is what the first cut would contain. Published here for review before any file lands:

```yaml
# Spacebot values for bjw-s-labs/app-template chart.
# Overridden per-environment by the cluster repo's HelmRelease values.

defaultPodOptions:
  annotations:
    secret.reloader.stakater.com/reload: spacebot-secret
    configmap.reloader.stakater.com/reload: spacebot-config
  securityContext:
    runAsNonRoot: true
    runAsUser: 1000
    fsGroup: 1000
  # Uncomment if the GHCR image is private. The `ghcr-pull-secret` already
  # exists in the `ai` namespace (see cluster repo:
  # templates/config/kubernetes/apps/ai/ghcr-pull-secret.sops.yaml.j2).
  # The current ghcr.io/jrmatherly/spacebot image is public, so this is not
  # needed today. apollos-portal in the same namespace uses this pattern.
  # imagePullSecrets:
  #   - name: ghcr-pull-secret

controllers:
  spacebot:
    type: deployment           # Single-replica; embedded DBs don't support HA
    replicas: 1
    strategy: Recreate          # File-locked SQLite/redb; no rolling with shared PVC
    pod:
      terminationGracePeriodSeconds: 30
    containers:
      app:
        image:
          repository: ghcr.io/jrmatherly/spacebot
          tag: v0.4.1           # Overridden in cluster repo per release
          pullPolicy: IfNotPresent
        env:
          SPACEBOT_DIR: /data
          SPACEBOT_DEPLOYMENT: docker
          RUST_LOG: info
        envFrom:
          - secretRef:
              name: spacebot-secret
        probes:
          liveness:
            enabled: true
            custom: true
            spec:
              httpGet:
                path: /api/health
                port: 19898
              periodSeconds: 30
              failureThreshold: 3
              timeoutSeconds: 5
          readiness:
            enabled: true
            custom: true
            spec:
              httpGet:
                path: /api/health
                port: 19898
              periodSeconds: 10
              failureThreshold: 3
              timeoutSeconds: 5
          startup:
            enabled: true
            custom: true
            spec:
              httpGet:
                path: /api/health
                port: 19898
              initialDelaySeconds: 5
              periodSeconds: 5
              failureThreshold: 30  # 155s total headroom for DB migrations
              timeoutSeconds: 5
        resources:
          requests:
            cpu: 250m
            memory: 256Mi
          limits:
            cpu: 1000m
            memory: 1Gi

service:
  app:
    controller: spacebot
    ports:
      http:
        port: 19898
        protocol: HTTP
      metrics:
        port: 9090
        protocol: HTTP

persistence:
  data:
    enabled: true
    type: persistentVolumeClaim
    accessMode: ReadWriteOnce
    size: 5Gi
    globalMounts:
      - path: /data

serviceMonitor:
  app:
    enabled: true
    serviceName: spacebot
    endpoints:
      - port: metrics
        scheme: http
        path: /metrics
        interval: 30s
```

Deliberately **not** in this values file:
- **Ingress (HTTPRoute).** Belongs in the cluster repo's `httproute.yaml.j2` because ingress config is cluster-specific (gateway name, hostname, domain).
- **ConfigMap/Secret content.** The chart references `spacebot-secret` and `spacebot-config` by name; the cluster repo's `secret.sops.yaml.j2` and `configmap.yaml.j2` provide their contents.
- **CiliumNetworkPolicy.** Not supported by app-template cleanly; belongs in the cluster repo.
- **Active `imagePullSecrets`.** The line is commented out because the current GHCR image is public. Uncomment when that changes. The `ghcr-pull-secret` secret is already provisioned in the `ai` namespace, so no cluster-side work is needed to activate it — just the one-line uncomment in values.

## Decisions baked into the recommendation

1. **Deployment (not StatefulSet).** Embedded databases use file locking → no multi-replica. `replicas: 1` plus `strategy: Recreate` prevents overlapping pods during rollout.
2. **`Recreate` strategy over RollingUpdate.** A new pod cannot mount the `ReadWriteOnce` PVC while the old pod holds it, so rolling update would deadlock. Recreate guarantees clean handoff (brief downtime during pod swap).
3. **Three probes, all point at `/api/health`.** Startup gives 155s grace for migrations; liveness and readiness are standard.
4. **Metrics port in the Service but no custom values for the chart — the ServiceMonitor block handles scraping.**
5. **Image pinning defers to cluster repo.** The chart's `values.yaml` pins `v0.4.1` as a *default*; Flux values in the cluster repo override this per release. Keeps the chart versioning and image versioning independent.
6. **Resources are conservative.** Spacebot's actual footprint from fly.toml is `shared-cpu-2x / 1gb` — requests of `250m / 256Mi` with `1000m / 1Gi` limits match that sizing and let the cluster scheduler pack efficiently.

## Open questions deferred to implementation time

These are intentionally not decided by this doc; they surface when someone picks up the scaffolding work:

1. **Chart publishing pipeline.** Should the chart publish automatically via `.github/workflows/release.yml` alongside the container image, or via a separate workflow? Recommendation: extend the existing workflow.
2. **Chart testing.** `helm lint` + `helm template` in CI? `kubeconform` against rendered manifests? Matches the cluster repo's own validation pipeline.
3. **How to test locally.** Docs on `helm install spacebot deploy/helm/spacebot/ --values local-values.yaml` against minikube. Needs a `local-values.yaml` example that uses an emptyDir instead of PVC.
4. **Whether `/api/idle` should gate pod termination** via `preStop` hook polling until idle returns 200. The design-docs pattern supports this; implementation would need the pod's terminationGracePeriod set high enough.
5. **`values.schema.json` for values validation.** Extra 50-100 lines but catches misconfigurations at `helm install` time.
6. **Init container for migrations.** Currently migrations run in the main container at startup. If migrations grow to take >2 min, move them to an init container so probes don't flake during migration. Not needed yet.

## Handoff to cluster repo (Option 1a)

Once `deploy/helm/spacebot/values.yaml` lands in this repo, the handoff is:

1. In the cluster repo, run `/cluster-deploy scaffold`. The skill emits the file skeleton for `templates/config/kubernetes/apps/ai/spacebot/app/`.
2. `ocirepository.yaml.j2` points at `oci://ghcr.io/bjw-s-labs/helm/app-template` (NOT at a wrapper chart), pinned to version `4.6.2`. Matches litellm's pattern exactly.
3. `helmrelease.yaml.j2`'s `spec.values:` is populated from this repo's `values.yaml` — either inline-copied or referenced via a Flux `substituteFrom` / a ConfigMap sourced from git.
4. Cluster-only additions in the same Flux app directory: `httproute.yaml.j2` (ingress), `secret.sops.yaml.j2` (SOPS-encrypted env vars for the `spacebot-secret` the values file references), `configmap.yaml.j2` (non-secret config for `spacebot-config`), `ciliumnetworkpolicy.yaml.j2` (network egress rules).
5. Flux reconciles → cluster picks up app-template + values → Spacebot deploys.

Two repos, one clean handoff, no chart-publishing pipeline, no version-sync problem.

### If Option 1b is chosen instead

Only the first two steps change: chart-publishing CI job pushes `oci://ghcr.io/jrmatherly/spacebot-chart:<version>`, and the cluster's `ocirepository.yaml.j2` points at the wrapper. Everything else (httproute, secrets, cilium) is identical.

## Explicitly excluded from the decision

The scaffold scope stopped short of:

- The chart files themselves (delivered as a follow-up change).
- The cluster repo side (owned by `/cluster-deploy`).
- Any actual deployment.

## Changelog

- **2026-04-16 (initial draft):** Option 1 recommended publishing a thin wrapper chart.
- **2026-04-16 (post-review refinement):** Split Option 1 into 1a (values bundle, direct consumption) and 1b (wrapper chart). Changed primary recommendation to 1a after verifying the cluster's actual pattern — no `ai` namespace app publishes a wrapper. Added `imagePullSecrets` guidance (commented out, activated only if image becomes private). Updated handoff section to reflect the 1a flow.
- **2026-04-17:** Published to `docs/design-docs/` from `.scratchpad/completed/` so the `deploy/helm/spacebot/README.md` reference points at a tracked file.
