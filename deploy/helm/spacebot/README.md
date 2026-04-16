# Spacebot Helm Values

Kubernetes deployment values for Spacebot. This directory does **not** ship a Helm chart. It ships values for the upstream `bjw-s-labs/app-template` chart, which every app in the target cluster uses.

## Why values-only (not a wrapper chart)

The target Talos cluster (`ai-k8s/talos-ai-cluster`) consumes `app-template` directly in every `HelmRelease` (litellm, langfuse, apollos-portal, cluster-docs, ~15 apps). Publishing a Spacebot-specific wrapper chart would make Spacebot the odd app out and add a chart-publishing pipeline (CI job, OCI push, version sync with the image tag) for no benefit. Instead, the cluster's `HelmRelease` references `app-template` and supplies the values from this file.

See `.scratchpad/k8s-helm-scaffold.md` in this repo for the full decision record.

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
- A `ConfigMap` named `spacebot-config` if you want Stakater Reloader to restart the pod on config changes (optional).
- A `StorageClass` capable of fulfilling a 5 Gi `ReadWriteOnce` PVC.
- Ingress configured externally (the cluster repo provisions a `HTTPRoute` at `httproute.yaml.j2`; this chart does not provision ingress).

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
- Ports: `19898` (HTTP API + UI), `9090` (Prometheus metrics, feature-gated).
- Volume: single `persistentVolumeClaim` at `/data` holding all three databases.
- Probes: liveness / readiness / startup all hit `/api/health`. Startup gives 150 s grace for SQLite migrations.
- Resources: 250 m / 256 Mi requests, 1000 m / 1 Gi limits (matches the fly.io baseline).
- Security: non-root (UID 1000), `fsGroup: 1000`.
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

The values-only approach is right while Spacebot deploys to a single cluster with this deployment pattern. Switch to a publishable wrapper chart (see `.scratchpad/k8s-helm-scaffold.md` Option 1b) only when a second independent operator wants to deploy Spacebot outside this cluster. Until then, the values bundle is the lower-overhead option.

## References

- Decision record: `.scratchpad/k8s-helm-scaffold.md`
- Upstream chart: [bjw-s-labs/helm-charts](https://github.com/bjw-s-labs/helm-charts), `app-template`
- Cluster repo pattern: `/Users/jason/dev/ai-k8s/talos-ai-cluster/templates/config/kubernetes/apps/ai/` (litellm, langfuse)
- Cluster deploy workflow: `/cluster-deploy` skill
