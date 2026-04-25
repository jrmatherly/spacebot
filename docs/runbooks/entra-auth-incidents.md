# Entra Auth Incident Response Runbook

Operator guide for diagnosing and recovering from Entra ID auth incidents. Source: SOC 2 CC7.4 (incident response).

## Triggers

| Event | Threshold | Severity | Page |
|---|---|---|---|
| Failed-auth rate (per principal) | 20/min | warn | #sec-alerts |
| Failed-auth rate (per source IP) | 50/min | warn | #sec-alerts |
| JWKS signature anomaly (`JwksUnreachable` sustained) | 1 min | page | oncall |
| Audit chain integrity fails (`/api/admin/audit/verify`) | any | page | oncall |
| `SpacebotAdmin` role added to any user | any | info | audit-only |
| Graph `GroupMember.Read.All` 5xx rate | 20% | warn | #sec-alerts |
| Token lifetime exceeded on cached refresh token | any | info | audit-only |

## Metric names (Prometheus)

- `spacebot_auth_failures_total{branch,reason}` (Phase 0 + Phase 1).
- `spacebot_audit_export_rows_total` (Phase 5).
- `spacebot_authz_skipped_total{reason}` (Phase 4 PR 1, registered at `src/telemetry/registry.rs`). Counts pool-None bypasses, admin overrides, and similar authz short-circuits. Does NOT count true denials.

## Response playbooks

### Failed-auth spike

1. Query `/api/admin/audit?action=auth_failure&limit=100` for the principal.
2. Contact the principal (email or Slack) to confirm whether they are trying to sign in.
3. If the principal denies activity: invalidate their refresh tokens via Entra admin console, suspend their `SpacebotAdmin` role if applicable, open an investigation ticket.
4. Post-mortem within 48 hours.

### JWKS signature anomaly

1. Check Microsoft service health dashboard.
2. Check `spacebot_auth_failures_total{reason="jwks_unreachable"}`.
3. Inspect `/api/admin/audit?action=auth_failure&reason=jwks_unreachable`.
4. If Microsoft-side: communicate to customers; degrade gracefully (the daemon already fails 503, not 500).
5. If local: check egress rules from the pod to `login.microsoftonline.com`; verify the JWKS cache has a fallback.

`src/auth/jwks.rs` configures jwt-authorizer with `refresh_interval = 0` and `retry_interval = 10s`. An unknown `kid` triggers an immediate JWKS refetch, so keys rotated upstream are picked up on the first request after rotation. The retry interval rate-limits hits when the JWKS endpoint is itself failing.

### Audit chain tamper

**Treat as compromise.** Steps:

1. Snapshot `audit_events` to an immutable sink immediately.
2. Revoke all admin tokens in Entra (they are the principals with DB write access via the admin endpoints).
3. Rotate `SecretsStore` credentials (Graph client_secret, sidecar keys).
4. Contact SecOps lead and engage the incident commander.
5. Do not replay; preserve the current state for forensics.

### Group-membership anomaly

1. `/api/admin/audit?action=admin_claim_resource` for recent claims.
2. Cross-reference with the Entra audit log (Entra admin > Monitoring > Sign-in logs).
3. If unauthorized: revoke, rotate, investigate.

### Orphaned-ownership accumulation

1. Run `GET /api/admin/orphans` (admin token).
2. For `MissingOwnership` rows: use `spacebot entra admin claim-resource` to assign ownership.
3. For `StaleOwnership` rows: review with the team that owned the missing agent before deleting; the orphan-sweep is currently report-only and does not auto-delete.

## Microsoft-side dependencies

- Entra ID: `status.office.com`, Microsoft service health (Azure portal).
- Microsoft Graph: `graph.microsoft.com/endpoints` status page.
- Spacebot's SOC 2 report depends on Microsoft's SOC 2 Type II; see `docs/security/third-party-assessments/microsoft-entra.md`.

## Cross-references

- `docs/design-docs/entra-architecture-diagram.md`: end-to-end auth flow.
- `docs/design-docs/entra-audit-log.md`: chain verification procedure and export modes.
- `docs/runbooks/entra-change-management.md`: pre-prod tenant procedure.
