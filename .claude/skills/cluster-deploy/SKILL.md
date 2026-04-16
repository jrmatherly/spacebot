---
name: cluster-deploy
description: Guide for deploying Spacebot into the Talos Kubernetes cluster via Flux GitOps. Use when the user wants to create or update Spacebot's Kubernetes manifests, add Spacebot to the cluster, scaffold the Flux app structure, configure secrets for the cluster, set up ingress/networking, or prepare Spacebot for Kubernetes deployment. Also triggers on "deploy to cluster", "add to k8s", "create helm release", "flux app", or "cluster manifest".
disable-model-invocation: true
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

Spacebot deploys as a single-replica Deployment (not a StatefulSet, because the embedded databases use file locking that doesn't support multi-replica). The full resource set:

```
templates/config/kubernetes/apps/ai/spacebot/
├── ks.yaml.j2                           # Flux Kustomization entry point
└── app/
    ├── kustomization.yaml.j2            # Resource list
    ├── deployment.yaml.j2               # Pod spec with probes, volumes, env
    ├── service.yaml.j2                  # ClusterIP service (port 19898)
    ├── httproute.yaml.j2                # Envoy Gateway ingress
    ├── pvc.yaml.j2                      # Persistent storage for /data
    ├── secret.sops.yaml.j2             # Encrypted credentials
    ├── configmap.yaml.j2               # Non-secret configuration
    ├── servicemonitor.yaml.j2          # Prometheus scrape config
    └── ciliumnetworkpolicy.yaml.j2     # Network access rules
```

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

### app/deployment.yaml.j2

Key decisions for the pod spec:

- **Image**: `ghcr.io/spacedriveapp/spacebot:#{ spacebot_version }#` (or internal fork registry)
- **Port**: 19898 (API + web UI)
- **Volume mount**: `/data` from a PVC
- **Environment**: `SPACEBOT_DIR=/data`, `SPACEBOT_DEPLOYMENT=docker`
- **Probes**: All three point at `/api/health` on port 19898
  - Startup: `initialDelaySeconds: 5`, `periodSeconds: 5`, `failureThreshold: 30`
  - Liveness: `periodSeconds: 30`, `failureThreshold: 3`
  - Readiness: `periodSeconds: 10`, `failureThreshold: 3`
- **Resources**: Start with `requests: 256Mi/250m`, `limits: 1Gi/1000m`, tune from metrics
- **Security context**: `runAsNonRoot: true`, `readOnlyRootFilesystem: false` (Spacebot writes to `/data`)

### app/service.yaml.j2

```yaml
---
apiVersion: v1
kind: Service
metadata:
  name: spacebot
  labels:
    app.kubernetes.io/name: spacebot
spec:
  type: ClusterIP
  ports:
    - name: http
      port: 19898
      targetPort: 19898
      protocol: TCP
    - name: metrics
      port: 9090
      targetPort: 9090
      protocol: TCP
  selector:
    app.kubernetes.io/name: spacebot
```

### app/pvc.yaml.j2

```yaml
---
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: spacebot-data
spec:
  accessModes:
    - ReadWriteOnce
  resources:
    requests:
      storage: "#{ spacebot_storage_size | default('5Gi') }#"
```

### app/kustomization.yaml.j2

```yaml
---
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization
resources:
  - ./deployment.yaml
  - ./service.yaml
  - ./pvc.yaml
  - ./httproute.yaml
  - ./secret.sops.yaml
  - ./configmap.yaml
  - ./servicemonitor.yaml
  - ./ciliumnetworkpolicy.yaml
```

### Post-scaffold

Update the namespace kustomization to include Spacebot:

```
templates/config/kubernetes/apps/ai/kustomization.yaml.j2
```

Add `- ./spacebot/ks.yaml` under `resources:`.

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

### app/servicemonitor.yaml.j2

```yaml
---
apiVersion: monitoring.coreos.com/v1
kind: ServiceMonitor
metadata:
  name: spacebot
  labels:
    app.kubernetes.io/name: spacebot
spec:
  selector:
    matchLabels:
      app.kubernetes.io/name: spacebot
  endpoints:
    - port: metrics
      interval: 30s
      path: /metrics
```

Spacebot already exposes Prometheus metrics on port 9090 (see `METRICS.md`). The ServiceMonitor tells Prometheus where to find them.

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

## Migration from Fly.io

Spacebot currently runs on Fly.io (`fly.toml` in repo root). The transition plan:

1. Build and push the container image to GHCR (or use existing CI)
2. Create the cluster manifests using this skill
3. Run both Fly.io and K8s in parallel during validation
4. Switch DNS to the K8s ingress
5. Decommission the Fly.io deployment

The container image is identical. The `Dockerfile` already produces a portable image. The only differences are environment variables and the storage backend (Fly volume vs. K8s PVC).
