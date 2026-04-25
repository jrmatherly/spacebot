# SOC 2 Evidence Package: Entra ID Auth

All artifacts below are owned by the security team and refreshed on the cadence noted.

| # | Artifact | File | Owner | Refresh cadence |
|---|---|---|---|---|
| 1 | Architecture diagram | `docs/design-docs/entra-architecture-diagram.md` | Security team | On auth changes |
| 2 | Role × Resource × Action matrix | `docs/design-docs/entra-role-permission-matrix.md` | Security team | On role changes |
| 3 | User access matrix (generated) | Output of `scripts/generate-access-matrix.sh` | Security team | Quarterly |
| 4 | Audit log samples with principal attribution | Generated from `/api/admin/audit` | Security team | On request |
| 5 | JWKS rotation test | `tests/jwks_rotation.rs` | Eng team | Keeps passing in CI |
| 6 | Third-party risk: Microsoft Entra | `docs/security/third-party-assessments/microsoft-entra.md` | Security team | Annually |
| 7 | Incident response runbook | `docs/runbooks/entra-auth-incidents.md` | Security team | Annually |
| 8 | Change management (CODEOWNERS + runbook) | `.github/CODEOWNERS` + `docs/runbooks/entra-change-management.md` | Security team | On process changes |
| 9 | Data classification register | `docs/security/data-classification.md` | Security team | Quarterly |
| 10 | Pentest scope and report | `docs/security/pentest-scope.md` plus external reports | Security team | Annually |

## Chain verification for auditors

Run:

```bash
curl -H "Authorization: Bearer <admin-token>" \
    https://<deployment>/api/admin/audit/verify
```

Expect: `{"valid": true, "total_rows": N}`.

## Access review for auditors

Run:

```bash
curl -H "Authorization: Bearer <admin-token>" \
    "https://<deployment>/api/admin/access-review?format=csv" > review.csv
```

Expect: CSV with one row per user, including teams and `last_seen_at`.

## Orphaned-resource report

```bash
curl -H "Authorization: Bearer <admin-token>" \
    https://<deployment>/api/admin/orphans > orphans.json
```

Expected: empty list in a mature deployment (everything claimed). MissingOwnership rows in a freshly-rolled-out tenant are normal until Phase 9's `spacebot entra admin claim-resource` has been run for each pre-Entra resource.

## Cross-references

- `docs/design-docs/entra-architecture-diagram.md`
- `docs/design-docs/entra-app-registrations.md`
- `docs/design-docs/entra-audit-log.md`
- `docs/design-docs/entra-backfill-strategy.md`
- `docs/design-docs/postgres-migration.md` (roadmap)
- `docs/runbooks/entra-auth-incidents.md`
- `docs/runbooks/entra-change-management.md`
- `docs/security/data-classification.md`
- `docs/security/pentest-scope.md`
- `docs/security/third-party-assessments/microsoft-entra.md`
