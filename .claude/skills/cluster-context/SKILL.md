---
name: cluster-context
description: Reference knowledge about the Talos Kubernetes cluster that Spacebot deploys into. Use PROACTIVELY when working on containerization, Dockerfile changes, deployment configuration, health checks, environment variables, networking, storage, secrets, database connections, or any work that touches how Spacebot runs in production. Also use when the user mentions the cluster, Kubernetes, k8s, Talos, Flux, namespaces, pods, or asks about Spacebot's production environment.
---

# Cluster Context

Spacebot's production deployment target is a Talos Linux Kubernetes cluster managed with Flux GitOps. This skill provides the deployment context that informs decisions about containerization, configuration, networking, and service dependencies.

The cluster repository lives at `/Users/jason/dev/ai-k8s/talos-ai-cluster/`. When you need current values (versions, config), read from that repo rather than relying on what's written here. This skill provides the architecture and patterns, not pinned versions.

## Cluster Architecture

### Infrastructure Stack

| Layer | Technology | Notes |
|-------|-----------|-------|
| OS | Talos Linux | Immutable, API-driven, no SSH |
| CNI | Cilium | Network policies, service mesh |
| GitOps | Flux v2 + Flux Operator | All state declared in git |
| Ingress | Envoy Gateway | Gateway API, TLS termination |
| DNS | CoreDNS + ExternalDNS | Internal + external resolution |
| Certificates | cert-manager | Automated TLS via Let's Encrypt |
| Secrets | SOPS + age | Encrypted at rest in git |
| Monitoring | Prometheus + Grafana + Loki + Alloy + Tempo | Full observability stack |
| Database Operator | CloudNativePG (CNPG) | Managed PostgreSQL clusters |
| Container Registry | Spegel | In-cluster OCI mirror |
| Storage | vSphere CSI + NFS | RWO block + RWX file |

### Namespace Layout

Applications live in purpose-grouped namespaces. Spacebot will deploy into the `ai` namespace alongside other AI workloads:

| Namespace | Purpose | Key Services |
|-----------|---------|-------------|
| `ai` | AI applications | LiteLLM, OpenWebUI, Langfuse |
| `database` | Database clusters | CNPG PostgreSQL instances, Redis HA, SurrealDB, Milvus, Weaviate |
| `monitoring` | Observability | Prometheus, Grafana, Loki, Tempo, Alloy |
| `network` | Networking | Envoy Gateway, ExternalDNS, cert-manager |
| `flux-system` | GitOps | Flux controllers |
| `kube-system` | Core K8s | CoreDNS, Cilium, Spegel |

### Template System

The cluster repo uses Jinja2 templates with custom delimiters (not standard `{{ }}`):

| Delimiter | Purpose |
|-----------|---------|
| `#{ variable }#` | Variable substitution |
| `#% if condition %#` | Block logic |
| `#\| comment \|#` | Template comments |

Templates render from `templates/config/` into `kubernetes/`, `talos/`, and `bootstrap/` directories. The render pipeline: `cluster.yaml` (values) + `.j2` templates -> `mise run configure` -> rendered YAML -> SOPS encryption -> Flux reconciliation.

### Flux GitOps Pattern

Every app follows this file structure in the cluster repo:

```
templates/config/kubernetes/apps/<namespace>/<app-name>/
├── ks.yaml.j2                    # Flux Kustomization
└── app/
    ├── kustomization.yaml.j2     # Kustomize resource list
    ├── helmrelease.yaml.j2       # Helm chart config (if using a chart)
    ├── ocirepository.yaml.j2     # Chart source (if OCI)
    ├── deployment.yaml.j2        # Direct manifests (if not using Helm)
    ├── service.yaml.j2
    └── secret.sops.yaml.j2       # SOPS-encrypted secrets
```

Variables flow from `cluster.yaml` through the Jinja2 templates. The 8-location checklist for new variables:
1. `cluster.sample.yaml` (documented entry)
2. `cluster.yaml` (live value, gitignored)
3. `.taskfiles/template/resources/cluster.schema.cue` (CUE type constraint)
4. `templates/scripts/apps/<app>.py` (defaults in app config module)
5. `.github/tests/public.yaml` (test fixture)
6. `.yaml.j2` templates (usage)
7. `mise run generate-schema` (regenerate JSON Schema)
8. `cue vet` against test fixtures

## Spacebot's Deployment Profile

### Container Image

Spacebot builds a multi-stage Docker image (see `Dockerfile`). The production image exposes:

- **Port 19898** — API server (HTTP)
- **Port 9090** — Prometheus metrics (when enabled)
- **Data directory** — `/data` (SQLite, LanceDB, redb, config)
- **Environment** — `SPACEBOT_DIR=/data`, `SPACEBOT_DEPLOYMENT=docker`

### Required Cluster Services

Spacebot depends on these cluster-provided services:

| Service | Purpose | Connection |
|---------|---------|-----------|
| LiteLLM | LLM routing proxy | HTTP API in `ai` namespace |
| PostgreSQL (CNPG) | Relational data (if migrated from SQLite) | TCP 5432 in `database` namespace |
| Envoy Gateway | External HTTPS ingress | Gateway API HTTPRoute |
| cert-manager | TLS certificates | ClusterIssuer reference |
| ExternalDNS | DNS record automation | Annotation-driven |
| Prometheus | Metrics scraping | ServiceMonitor CRD |
| Loki + Alloy | Log aggregation | Stdout/stderr collection |

### Storage Requirements

Spacebot uses three embedded databases that write to the data directory:

| Database | Type | Storage Need |
|----------|------|-------------|
| SQLite | Relational | RWO persistent volume |
| LanceDB | Vector search | Same volume (subdirectory) |
| redb | Key-value | Same volume (subdirectory) |

A single PVC mounted at `/data` covers all three. Use the vSphere CSI StorageClass for RWO block storage. Size recommendation: start at 5Gi, monitor with Prometheus.

### Health Checks

The API server responds to `GET /api/health` on port 19898. Use this for:
- Kubernetes liveness probe
- Kubernetes readiness probe
- Startup probe (Spacebot loads prompts and initializes databases on start)

### Networking

Spacebot needs:
- **Inbound**: HTTPS via Envoy Gateway (Gateway API HTTPRoute) for the web UI and API
- **Outbound**: HTTP to LiteLLM (in-cluster), HTTPS to external LLM providers (if direct), IMAP/SMTP for email channels
- **CiliumNetworkPolicy**: Restrict traffic to required services

For the full cluster topology, service dependency map, and app configuration patterns, read `references/services.md` within this skill directory.
