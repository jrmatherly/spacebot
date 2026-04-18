---
name: cluster-deploy
description: Guide for deploying Spacebot into the Talos Kubernetes cluster via Flux GitOps. Use when the user wants to create or update Spacebot's Kubernetes manifests, add Spacebot to the cluster, scaffold the Flux app structure, configure secrets for the cluster, set up ingress/networking, or prepare Spacebot for Kubernetes deployment. Also triggers on "deploy to cluster", "add to k8s", "create helm release", "flux app", or "cluster manifest".
---

# Cluster Deploy

Deploy Spacebot into the Talos Kubernetes cluster following the project's Flux GitOps conventions. This skill guides the creation of manifests in the **cluster repository** (not the Spacebot repo).

Before using this skill, read `/cluster-context` for the full cluster architecture reference.

## Arguments

```
/cluster-deploy [step]
```

Steps: `scaffold`, `secrets`, `ingress`, `monitoring`, `validate`, or omit for the full guided workflow.

## Prerequisites

The cluster repository must be available at `/Users/jason/dev/ai-k8s/talos-ai-cluster/`. Verify:

```bash
ls /Users/jason/dev/ai-k8s/talos-ai-cluster/cluster.yaml
```

If unavailable, tell the user and stop. The manifests belong in that repo.

## Deployment Architecture

Spacebot deploys via the **canonical `ai`-namespace pattern**: a `HelmRelease` that consumes the `bjw-s-labs/app-template` chart, with all customization expressed as structured values. No per-app templates. 12 of 14 `ai`-namespace apps use this pattern (litellm, apollos-portal, cluster-docs, open-webui, etc.) — Spacebot follows suit.

Spacebot's deployment shape:
- **Deployment**, not StatefulSet — embedded databases use file locking, can't be multi-replica
- **`strategy: Recreate`** — new pod can't mount the `ReadWriteOnce` PVC while the old one holds it
- **`replicas: 1`**

The resource set in the cluster repo:

```
templates/config/kubernetes/apps/ai/spacebot/
├── ks.yaml.j2                           # Flux Kustomization entry point
└── app/
    ├── kustomization.yaml.j2            # Kustomize resource list
    ├── ocirepository.yaml.j2            # app-template chart source
    ├── helmrelease.yaml.j2              # HelmRelease with all Spacebot values
    ├── configmap.yaml.j2                # Non-secret config
    ├── secret.sops.yaml.j2              # SOPS-encrypted credentials
    ├── httproute.yaml.j2                # Envoy Gateway ingress
    └── ciliumnetworkpolicy.yaml.j2      # Network policy
```

**No `deployment.yaml.j2`, `service.yaml.j2`, `pvc.yaml.j2`, or `servicemonitor.yaml.j2`.** app-template generates all of those from `HelmRelease.spec.values`.

## Step 1: Scaffold the Flux App (`/cluster-deploy scaffold`)

Create the base file structure in the cluster repo. Work in `/Users/jason/dev/ai-k8s/talos-ai-cluster/`.

### ks.yaml.j2

```yaml
#% if spacebot_enabled %#
---
apiVersion: kustomize.toolkit.fluxcd.io/v1
kind: Kustomization
metadata:
  name: spacebot
spec:
  interval: 1h
  dependsOn:
    - name: coredns
      namespace: kube-system
    - name: litellm
      namespace: ai
  path: ./kubernetes/apps/ai/spacebot/app
  postBuild:
    substituteFrom:
      - name: cluster-secrets
        kind: Secret
  prune: true
  sourceRef:
    kind: GitRepository
    name: flux-system
    namespace: flux-system
  targetNamespace: ai
  wait: true
  timeout: 15m
#% endif %#
```

### app/ocirepository.yaml.j2

```yaml
#% if spacebot_enabled %#
---
apiVersion: source.toolkit.fluxcd.io/v1
kind: OCIRepository
metadata:
  name: spacebot
spec:
  interval: 15m
  layerSelector:
    mediaType: application/vnd.cncf.helm.chart.content.v1.tar+gzip
    operation: copy
  ref:
    tag: 4.6.2
  url: oci://ghcr.io/bjw-s-labs/helm/app-template
#% endif %#
```

The `tag:` pins the `app-template` chart version. Update when bjw-s ships a new major version and the cluster has been migrated.

### app/helmrelease.yaml.j2

```yaml
#% if spacebot_enabled %#
---
apiVersion: helm.toolkit.fluxcd.io/v2
kind: HelmRelease
metadata:
  name: spacebot
spec:
  timeout: 15m
  chartRef:
    kind: OCIRepository
    name: spacebot
  interval: 1h
  values:
    defaultPodOptions:
      annotations:
        configmap.reloader.stakater.com/reload: spacebot-config
        secret.reloader.stakater.com/reload: spacebot-secret
      securityContext:
        runAsNonRoot: true
        runAsUser: 1000
        fsGroup: 1000
      # Uncomment if the GHCR image is private. ghcr-pull-secret already
      # exists in the `ai` namespace.
      # imagePullSecrets:
      #   - name: ghcr-pull-secret
    controllers:
      spacebot:
        type: deployment
        replicas: 1
        strategy: Recreate
        pod:
          terminationGracePeriodSeconds: 30
        containers:
          app:
            image:
              repository: ghcr.io/#{ spacebot_image_owner }#/spacebot
              tag: #{ spacebot_version }#
            env:
              SPACEBOT_DIR: /data
              SPACEBOT_DEPLOYMENT: docker
              RUST_LOG: info
            envFrom:
              - secretRef:
                  name: spacebot-secret
              - configMapRef:
                  name: spacebot-config
            probes:
              liveness:
                enabled: true
                custom: true
                spec:
                  httpGet: { path: /api/health, port: 19898 }
                  periodSeconds: 30
                  failureThreshold: 3
                  timeoutSeconds: 5
              readiness:
                enabled: true
                custom: true
                spec:
                  httpGet: { path: /api/health, port: 19898 }
                  periodSeconds: 10
                  failureThreshold: 3
                  timeoutSeconds: 5
              startup:
                enabled: true
                custom: true
                spec:
                  httpGet: { path: /api/health, port: 19898 }
                  initialDelaySeconds: 5
                  periodSeconds: 5
                  failureThreshold: 30
                  timeoutSeconds: 5
            resources:
              requests: { cpu: 250m, memory: 256Mi }
              limits: { cpu: 1000m, memory: 1Gi }
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
        size: "#{ spacebot_storage_size | default('5Gi') }#"
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
#% endif %#
```

### app/kustomization.yaml.j2

```yaml
#% if spacebot_enabled %#
---
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization
resources:
  - ./ocirepository.yaml
  - ./helmrelease.yaml
  - ./configmap.yaml
  - ./secret.sops.yaml
  - ./httproute.yaml
  - ./ciliumnetworkpolicy.yaml
#% endif %#
```

`ocirepository.yaml` and `helmrelease.yaml` are both listed — they reconcile together.

### Post-scaffold

Update the namespace kustomization to include Spacebot:

```
templates/config/kubernetes/apps/ai/kustomization.yaml.j2
```

Add `- ./spacebot/ks.yaml` under `resources:`.

### Why this shape, not raw manifests

An earlier version of this skill showed `deployment.yaml.j2`, `service.yaml.j2`, `pvc.yaml.j2`, and `servicemonitor.yaml.j2` as separate files. That pattern is **wrong for this cluster** — zero `ai`-namespace apps deploy that way. Every app uses `HelmRelease + OCIRepository + app-template + values`. Writing raw manifests would:

- Diverge from 12 working apps' patterns (litellm, apollos-portal, cluster-docs, etc.)
- Force manual maintenance of fields app-template handles (pod security context defaults, labels, ServiceMonitor wiring, reloader annotations)
- Break when K8s API versions shift (app-template handles migrations)

If a specific app genuinely needs shapes app-template cannot express, that's when to consider an upstream chart (pattern 2 in `/cluster-context`) or raw manifests (pattern 3). Spacebot does not need either.

## Step 2: Configure Secrets (`/cluster-deploy secrets`)

Create `app/secret.sops.yaml.j2` with all sensitive values:

```yaml
---
apiVersion: v1
kind: Secret
metadata:
  name: spacebot-secret
stringData:
  LITELLM_API_KEY: "#{ spacebot_litellm_api_key }#"
  DISCORD_TOKEN: "#{ spacebot_discord_token }#"
  SLACK_TOKEN: "#{ spacebot_slack_token }#"
```

Add the corresponding variables to `cluster.yaml` and `cluster.sample.yaml`. Follow the 8-location checklist from `/cluster-context` for each new variable.

Reference the secret in the deployment via `envFrom`:

```yaml
envFrom:
  - secretRef:
      name: spacebot-secret
```

Add a reloader annotation so config changes trigger a rolling restart:

```yaml
podAnnotations:
  secret.reloader.stakater.com/reload: spacebot-secret
  configmap.reloader.stakater.com/reload: spacebot-config
```

## Step 3: Configure Ingress (`/cluster-deploy ingress`)

### app/httproute.yaml.j2

```yaml
#% if spacebot_enabled %#
---
apiVersion: gateway.networking.k8s.io/v1
kind: HTTPRoute
metadata:
  name: spacebot
spec:
  parentRefs:
    - name: envoy-external
      namespace: network
      sectionName: https
  hostnames:
    - "#{ spacebot_subdomain }#.${SECRET_DOMAIN}"
  rules:
    - matches:
        - path:
            type: PathPrefix
            value: /
      backendRefs:
        - name: spacebot
          port: 19898
#% endif %#
```

The `${SECRET_DOMAIN}` variable comes from Flux post-build substitution (cluster-secrets), not Jinja2. ExternalDNS is configured at the Gateway level, not on individual HTTPRoutes.

## Step 4: Configure Monitoring (`/cluster-deploy monitoring`)

**Already handled in Step 1.** The `serviceMonitor:` values block inside `helmrelease.yaml.j2` tells app-template to generate a `ServiceMonitor` pointing at the `metrics` port (9090) at path `/metrics`. Do NOT add a separate `servicemonitor.yaml.j2` file — that would create a second, duplicate ServiceMonitor.

If you need additional Prometheus resources beyond the auto-generated ServiceMonitor (e.g., `PrometheusRule` for alerts, `PodMonitor` for sidecar metrics), add them as separate files in `app/` and list them in `kustomization.yaml.j2`. Spacebot exposes metrics on port 9090; see `METRICS.md` for the metric surface.

## Step 5: Network Policy (`/cluster-deploy network`)

### app/ciliumnetworkpolicy.yaml.j2

```yaml
#% if spacebot_enabled and network_policies_enabled %#
---
apiVersion: cilium.io/v2
kind: CiliumNetworkPolicy
metadata:
  name: spacebot
spec:
  endpointSelector:
    matchLabels:
      app.kubernetes.io/name: spacebot
  enableDefaultDeny:
    ingress: #{ cnp_enforce_ai | lower }#
    egress: #{ cnp_enforce_ai | lower }#
  ingress:
    #| From Envoy Gateway (external access) |#
    - fromEndpoints:
        - matchLabels:
            app.kubernetes.io/name: envoy
            gateway.envoyproxy.io/owning-gateway-name: envoy-external
            gateway.envoyproxy.io/owning-gateway-namespace: network
      toPorts:
        - ports:
            - port: "19898"
              protocol: TCP
  egress:
    #| To LiteLLM (LLM completions) |#
    - toEndpoints:
        - matchLabels:
            app.kubernetes.io/name: litellm
      toPorts:
        - ports:
            - port: "4000"
              protocol: TCP
    #| DNS resolution |#
    - toEndpoints:
        - matchLabels:
            k8s-app: kube-dns
      toPorts:
        - ports:
            - port: "53"
              protocol: UDP
            - port: "53"
              protocol: TCP
#% endif %#
```

Adjust egress rules based on which services Spacebot uses. Add database egress (CNPG, Redis) if those dependencies are added later. The `cnp_enforce_ai` variable controls whether the policy is enforced or audit-only.

## Step 6: App Config Module

Create `templates/scripts/apps/spacebot.py` following the registry pattern:

```python
"""Spacebot agent harness configuration."""
from typing import Any
from .base import AppConfig, _require_field


class SpacebotConfig(AppConfig):
    name = "Spacebot"
    enabled_flag = "spacebot_enabled"

    @staticmethod
    def defaults() -> dict[str, Any]:
        return {
            "spacebot_enabled": False,
            "spacebot_version": "",
            "spacebot_subdomain": "",
            "spacebot_storage_size": "5Gi",
            "spacebot_litellm_api_key": "",
            "spacebot_discord_token": "",
            "spacebot_slack_token": "",
        }

    @staticmethod
    def validate(data: dict[str, Any]) -> None:
        if data.get("spacebot_enabled"):
            _require_field(data, "spacebot_version", "Spacebot")
            _require_field(data, "spacebot_subdomain", "Spacebot")
```

## Step 7: Validate (`/cluster-deploy validate`)

After creating all manifests, validate from the cluster repo:

```bash
cd /Users/jason/dev/ai-k8s/talos-ai-cluster
mise run configure
```

This runs the full pipeline: CUE validation, template rendering, SOPS encryption, kubeconform, and talhelper. Fix any errors before committing.

## Cluster.yaml Variables

Summary of all variables Spacebot introduces:

| Variable | Purpose | Required |
|----------|---------|----------|
| `spacebot_enabled` | Enable/disable deployment | Yes |
| `spacebot_version` | Container image tag | Yes (when enabled) |
| `spacebot_subdomain` | Subdomain prefix (full hostname: `<subdomain>.${SECRET_DOMAIN}`) | Yes (when enabled) |
| `spacebot_storage_size` | PVC size | No (default: 5Gi) |
| `spacebot_litellm_api_key` | LiteLLM proxy key | Yes (when enabled) |
| `spacebot_discord_token` | Discord bot token | No |
| `spacebot_slack_token` | Slack bot token | No |

## Migration from Fly.io (historical)

Spacebot previously ran on Fly.io. The transition to the Talos cluster is complete; the historical Fly configs (`fly.toml`, `fly.staging.toml`) were decommissioned on 2026-04-18 and archived to `.scratchpad/backups/archive/` for reference to the original port, env, volume, and healthcheck choices.

The original transition plan was:

1. Build and push the container image to GHCR (or use existing CI).
2. Create the cluster manifests using this skill.
3. Run both Fly.io and K8s in parallel during validation.
4. Switch DNS to the K8s ingress.
5. Decommission the Fly.io deployment.

The container image was identical between platforms. The `Dockerfile` already produced a portable image. The only differences were environment variables and the storage backend (Fly volume vs. K8s PVC).
