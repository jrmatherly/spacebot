# Entra Auth Change Management

Source: SOC 2 CC8.1 (change management).

## Required for every PR that touches auth code

1. **Two-person review** via CODEOWNERS on `/src/auth/`, `/src/audit/`, `/src/secrets/`, `/src/api/server.rs`, `/src/api/admin_*.rs`, `/src/admin.rs`, `/migrations/global/`, `/docs/security/`, `/docs/runbooks/`.
2. **Security label** applied (`security` or `security/auth`).
3. **Rollback plan** in PR description (config flag, revert, or migration).
4. **Test coverage**: every new code path has a unit + integration test (see `.claude/rules/coding-discipline.md` TDD default).

GitHub branch protection enforces the required-reviewer count. CODEOWNERS declares which paths fall under the policy.

## Pre-prod tenant

- **Name**: `spacebot-preprod` in the Entra admin console.
- **Separate from production**: no shared secrets, no shared app registrations.
- **Access**: infrastructure team and SecOps only.
- **Purpose**: smoke tests before each auth-touching release.

## Release procedure for auth PRs

1. Merge to main with required approvals.
2. Deploy to pre-prod first.
3. Run the auth smoke suite against pre-prod.
4. Observe the audit log for 24 hours. Look for anomalies.
5. If clean: cut a release tag, deploy to production.
6. If not: revert in pre-prod and repeat.

## Credentials rotation

| Credential | Rotation | Triggered by |
|---|---|---|
| Web API client certificate | 90 days | Calendar |
| Graph `GroupMember.Read.All` client secret | 90 days | Calendar |
| `auth_token` (legacy static) | 30 days or on-demand | Calendar or suspicion |
| SPA redirect URIs | N/A (additive only) | New deployment |
| Desktop redirect URIs (50000-50009) | N/A (fixed set) | n/a |

Rotation playbooks land in `docs/runbooks/entra-credential-rotation.md` (future, not in this phase).

## Cross-references

- `.github/CODEOWNERS`: source-of-truth for which paths are under the policy.
- `docs/runbooks/entra-auth-incidents.md`: response procedures for sign-in failures and chain tamper.
- `docs/design-docs/entra-app-registrations.md`: Phase 1 schema for the three registrations.
