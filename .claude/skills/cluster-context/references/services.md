# Cluster Services Reference

Detailed reference for services available in the Talos cluster that Spacebot interacts with.

## Table of Contents

1. [LLM Routing (LiteLLM)](#llm-routing-litellm)
2. [Database Services](#database-services)
3. [Observability Stack](#observability-stack)
4. [Networking and Ingress](#networking-and-ingress)
5. [Secret Management](#secret-management)
6. [Vector Databases](#vector-databases)
7. [App Configuration Pattern](#app-configuration-pattern)

## LLM Routing (LiteLLM)

LiteLLM runs in the `ai` namespace as the central LLM proxy. Spacebot routes all LLM calls through it.

- **Service**: `litellm.ai.svc.cluster.local`
- **Port**: 4000 (HTTP)
- **Authentication**: API key via `LITELLM_API_KEY` environment variable
- **Features**: Model routing, rate limiting, spend tracking, fallbacks
- **OSS only**: The cluster runs the open-source edition. Enterprise features are not available.
- **Azure OpenAI**: Configured with deployment-based routing. API version pins live in `cluster.yaml` as `litellm_azure_*_api_version`.

When configuring Spacebot's LLM provider settings, point the base URL at the in-cluster LiteLLM service rather than directly at provider APIs.

## Database Services

### CloudNativePG (PostgreSQL)

CNPG manages PostgreSQL clusters in the `database` namespace. Each app that needs Postgres gets its own CNPG Cluster CR.

- **Pattern**: `<app>-db.database.svc.cluster.local:5432`
- **Auth**: Secrets auto-created by CNPG, referenced in app HelmRelease/deployment
- **Backups**: Configured per-cluster, usually to S3-compatible storage
- **pgvector**: Available for vector search if needed alongside or instead of LanceDB

If Spacebot migrates from SQLite to PostgreSQL, it would get its own CNPG cluster (e.g., `spacebot-db` in the `database` namespace).

### Redis HA

Redis with HAProxy runs in the `database` namespace for apps that need caching or pub/sub.

- **Service**: `redis-ha-haproxy.database.svc.cluster.local:6379`
- **Mode**: HAProxy over Sentinel (not raw Sentinel)

### SurrealDB

Multi-model database in the `database` namespace. Some AI apps use it for graph queries.

- **Service**: `surrealdb.database.svc.cluster.local:8000`

## Observability Stack

### Prometheus + Grafana

- **Prometheus**: `kube-prometheus-stack-prometheus.monitoring.svc.cluster.local:9090`
- **Grafana**: Accessible via Envoy Gateway ingress
- **ServiceMonitor**: Create a ServiceMonitor CRD to scrape Spacebot's `/metrics` endpoint (port 9090)

Spacebot already exposes Prometheus metrics (see `METRICS.md`). The cluster will scrape them automatically once a ServiceMonitor exists.

### Loki + Alloy

Alloy collects container stdout/stderr and ships to Loki. No app-side configuration needed. Spacebot's `tracing` output to stdout is captured automatically.

### Tempo

Distributed tracing backend. If Spacebot adds OpenTelemetry trace export, point the OTLP endpoint at Alloy (which forwards to Tempo).

## Networking and Ingress

### Envoy Gateway

Gateway API implementation. Spacebot needs an HTTPRoute to expose its web UI:

```yaml
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
    - "spacebot.${SECRET_DOMAIN}"
  rules:
    - matches:
        - path:
            type: PathPrefix
            value: /
      backendRefs:
        - name: spacebot
          port: 19898
```

### CiliumNetworkPolicy

Default-deny with explicit allows. Spacebot needs egress to:
- `ai` namespace (LiteLLM)
- `database` namespace (if using CNPG/Redis/SurrealDB)
- `kube-system` namespace (DNS)
- External HTTPS (LLM providers, email servers)

### cert-manager

TLS certificates via Let's Encrypt. Referenced in the Gateway listener configuration, not in individual app HTTPRoutes.

### ExternalDNS

Automatically creates DNS records from HTTPRoute annotations. No app-side config beyond the hostname in the HTTPRoute.

## Secret Management

Secrets follow the SOPS + age pattern:

1. Template: `secret.sops.yaml.j2` with `#{ variable }#` placeholders
2. Render: `mise run configure` substitutes values from `cluster.yaml`
3. Encrypt: SOPS encrypts with age public key
4. Deploy: Flux applies the encrypted secret, SOPS-decryption controller decrypts in-cluster

Spacebot secrets to manage:
- LLM API keys (or LiteLLM key if proxied)
- Database credentials (if CNPG)
- Discord/Slack/Telegram bot tokens
- IMAP/SMTP credentials
- Any other external service credentials

## Vector Databases

The cluster runs both Milvus and Weaviate in the `database` namespace. Spacebot currently uses embedded LanceDB, but if external vector search is needed:

- **Milvus**: `milvus.database.svc.cluster.local:19530` (standalone mode, gated by `milvus_enabled and milvus_mode == "standalone"`)
- **Weaviate**: `weaviate.database.svc.cluster.local:8080`

## App Configuration Pattern

Each app in the cluster has a Python config module at `templates/scripts/apps/<app>.py` that:

1. Declares an `AppConfig` subclass
2. Sets `enabled_flag` (the `cluster.yaml` variable that enables/disables the app)
3. Provides `defaults()` for all app-specific variables
4. Implements `validate()` for prerequisite checks

The module is auto-discovered by the registry. This is where Spacebot's cluster.yaml variables would be defined and validated.
