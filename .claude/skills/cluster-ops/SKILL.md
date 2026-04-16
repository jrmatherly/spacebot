---
name: cluster-ops
description: Debug, clean, redeploy, and validate Spacebot's running deployment in the Talos Kubernetes cluster. Use when the user wants to check pod status, view logs, clean up a failed deployment, force a redeployment, troubleshoot a crash loop, inspect cluster resources, or validate Spacebot is running correctly. Also triggers on "pod logs", "restart spacebot", "why is spacebot down", "flux reconcile", "clean deploy", "redeploy", or "cluster status".
disable-model-invocation: true
---

# Cluster Ops

Operational runbook for managing Spacebot's deployment in the Talos Kubernetes cluster. These commands run against the live cluster using `kubectl`, `flux`, and `talosctl`.

Before using this skill, read `/cluster-context` for the full cluster architecture reference.

## Arguments

```
/cluster-ops <operation>
```

Operations: `status`, `logs`, `clean`, `redeploy`, `debug`, `validate`

## Prerequisites

Verify cluster access before any operation:

```bash
kubectl get nodes 2>/dev/null && echo "cluster: connected" || echo "cluster: UNREACHABLE"
kubectl get namespace ai 2>/dev/null && echo "namespace: exists" || echo "namespace: MISSING"
```

If the cluster is unreachable, check:
1. `kubeconfig` file exists and is current
2. VPN or network access to the cluster
3. `talosctl health --talosconfig talosconfig` for node-level issues

## Status (`/cluster-ops status`)

Quick health check of all Spacebot resources:

```bash
# Pod status
kubectl get pods -n ai -l app.kubernetes.io/name=spacebot -o wide

# Flux kustomization status
flux get kustomization spacebot -n ai 2>/dev/null || echo "kustomization not found"

# HelmRelease status (if using Helm)
kubectl get helmrelease spacebot -n ai 2>/dev/null

# Service and endpoints
kubectl get svc spacebot -n ai
kubectl get endpoints spacebot -n ai

# PVC status
kubectl get pvc -n ai -l app.kubernetes.io/name=spacebot

# HTTPRoute
kubectl get httproute spacebot -n ai 2>/dev/null

# Recent events
kubectl get events -n ai --field-selector involvedObject.name=spacebot --sort-by='.lastTimestamp' | tail -20
```

Present results as a status dashboard:

```
## Spacebot Cluster Status

| Resource | Status | Details |
|----------|--------|---------|
| Pod | Running | 1/1 ready, uptime 3d |
| Service | Active | ClusterIP 10.x.x.x:19898 |
| PVC | Bound | 5Gi, 2.1Gi used |
| HTTPRoute | Attached | spacebot.example.com |
| Flux Kustomization | Applied | Last reconcile 5m ago |
```

## Logs (`/cluster-ops logs`)

```bash
# Recent logs (last 100 lines)
kubectl logs -n ai -l app.kubernetes.io/name=spacebot --tail=100

# Follow live logs
kubectl logs -n ai -l app.kubernetes.io/name=spacebot -f

# Previous container logs (after a crash)
kubectl logs -n ai -l app.kubernetes.io/name=spacebot --previous

# Filter by log level (Spacebot uses tracing crate format)
kubectl logs -n ai -l app.kubernetes.io/name=spacebot --tail=500 | grep -E "ERROR|WARN"
```

For historical logs, query Loki via Grafana. Alloy collects container stdout/stderr automatically.

## Clean (`/cluster-ops clean`)

Remove all Spacebot resources for a fresh redeployment. Adapted from the cluster project's clean workflow.

### Step 1: Discover resources

```bash
# Flux kustomizations
kubectl get kustomization -A 2>/dev/null | grep -i spacebot

# All resources by label
kubectl get all,configmap,secret,pvc,ciliumnetworkpolicy,httproute -n ai \
  -l app.kubernetes.io/name=spacebot 2>/dev/null

# Also search by name (some resources lack labels)
kubectl get all,configmap,secret,pvc,ciliumnetworkpolicy,httproute -n ai 2>/dev/null \
  | grep -i spacebot
```

### Step 2: Present findings and confirm

Show the user exactly what will be deleted. Ask for confirmation unless `--yes` was passed.

### Step 3: Suspend Flux

```bash
flux suspend kustomization spacebot -n ai
```

### Step 4: Delete in order

```bash
# 1. Workloads (stops pods)
kubectl delete deployment/spacebot -n ai --ignore-not-found

# 2. Networking
kubectl delete svc/spacebot httproute/spacebot ciliumnetworkpolicy/spacebot -n ai --ignore-not-found

# 3. Config
kubectl delete configmap/spacebot-config secret/spacebot-secret -n ai --ignore-not-found

# 4. Helm state (forces fresh install on redeploy)
kubectl delete helmrelease/spacebot -n ai --ignore-not-found
kubectl delete secret -n ai -l owner=helm,name=spacebot --ignore-not-found

# 5. Monitoring
kubectl delete servicemonitor/spacebot -n ai --ignore-not-found

# 6. PVC (LAST — data loss is irreversible, confirm separately)
echo "Delete PVC spacebot-data? This destroys all Spacebot data. [y/N]"
# Only if confirmed:
# kubectl delete pvc/spacebot-data -n ai
```

### Step 5: Verify clean

Re-run the discovery from Step 1. Report any remaining resources (stuck finalizers, etc.).

## Redeploy (`/cluster-ops redeploy`)

Resume Flux reconciliation after a clean, or force a reconciliation on the existing deployment:

```bash
# Resume if suspended
flux resume kustomization spacebot -n ai

# Force immediate reconciliation
flux reconcile kustomization spacebot -n ai --with-source

# Watch rollout
kubectl rollout status deployment/spacebot -n ai --timeout=120s
```

If the deployment fails to come up:
1. Check events: `kubectl describe pod -n ai -l app.kubernetes.io/name=spacebot`
2. Check image pull: verify the image tag exists in the registry
3. Check probes: startup probe may need a longer `failureThreshold` if Spacebot takes time to initialize databases

## Debug (`/cluster-ops debug`)

### CrashLoopBackOff

```bash
# Get the pod name
POD=$(kubectl get pod -n ai -l app.kubernetes.io/name=spacebot -o jsonpath='{.items[0].metadata.name}')

# Check exit code and reason
kubectl describe pod "$POD" -n ai | grep -A5 "Last State"

# Previous container logs
kubectl logs "$POD" -n ai --previous

# Common causes:
# - Missing /data volume mount (no PVC)
# - Database corruption (delete PVC and redeploy)
# - Missing environment variables (check secret)
# - OOM killed (increase memory limit)
```

### ImagePullBackOff

```bash
kubectl describe pod "$POD" -n ai | grep -A3 "Warning.*Failed"
# Check: is the image tag correct? Does the registry require auth?
# Spegel (in-cluster mirror) caches images — a new tag may need time to propagate
```

### Networking issues

```bash
# Can the pod resolve DNS?
kubectl exec -n ai "$POD" -- nslookup litellm.ai.svc.cluster.local

# Can the pod reach LiteLLM?
kubectl exec -n ai "$POD" -- wget -q -O- http://litellm.ai.svc.cluster.local:4000/health

# Check CiliumNetworkPolicy isn't blocking
kubectl get ciliumnetworkpolicy -n ai spacebot -o yaml

# Cilium connectivity test
cilium connectivity test --single-node
```

### Storage issues

```bash
# PVC status
kubectl get pvc spacebot-data -n ai -o yaml

# Disk usage inside the pod
kubectl exec -n ai "$POD" -- df -h /data
kubectl exec -n ai "$POD" -- du -sh /data/*
```

### Exec into pod

```bash
# Interactive shell (if the image has a shell)
kubectl exec -it -n ai "$POD" -- /bin/sh

# Or use a debug container if the image is distroless
kubectl debug -it -n ai "$POD" --image=busybox --target=spacebot
```

## Validate (`/cluster-ops validate`)

Post-deployment validation checklist:

```bash
# 1. Pod is running and ready
kubectl get pod -n ai -l app.kubernetes.io/name=spacebot -o jsonpath='{.items[0].status.phase}'
# Expected: Running

# 2. Health endpoint responds
kubectl exec -n ai "$POD" -- wget -q -O- http://localhost:19898/api/health
# Expected: 200 OK

# 3. Metrics endpoint responds
kubectl exec -n ai "$POD" -- wget -q -O- http://localhost:9090/metrics | head -5
# Expected: Prometheus text format

# 4. Ingress works (from outside the cluster)
curl -s "https://spacebot.example.com/api/health"
# Expected: 200 OK

# 5. Prometheus is scraping
kubectl exec -n monitoring -l app.kubernetes.io/name=prometheus -- \
  wget -q -O- 'http://localhost:9090/api/v1/targets' | grep spacebot
# Expected: state=up

# 6. Flux is reconciling
flux get kustomization spacebot -n ai
# Expected: Applied revision, no errors
```

## Quick Reference

| Task | Command |
|------|---------|
| Pod status | `kubectl get pods -n ai -l app.kubernetes.io/name=spacebot` |
| Tail logs | `kubectl logs -n ai -l app.kubernetes.io/name=spacebot -f` |
| Restart pod | `kubectl rollout restart deployment/spacebot -n ai` |
| Force reconcile | `flux reconcile kustomization spacebot -n ai --with-source` |
| Suspend Flux | `flux suspend kustomization spacebot -n ai` |
| Resume Flux | `flux resume kustomization spacebot -n ai` |
| Exec into pod | `kubectl exec -it -n ai <pod-name> -- /bin/sh` |
| Describe pod | `kubectl describe pod -n ai -l app.kubernetes.io/name=spacebot` |
| Events | `kubectl get events -n ai --sort-by='.lastTimestamp' \| grep spacebot` |
