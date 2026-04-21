# Role × Resource × Action Matrix

> SOC 2 CC6.3 evidence. Source: research §12 CC6.6, §11.1 Q3-Q4.

## Principals

- `SpacebotAdmin` — tenant-level admin (Entra app role).
- `SpacebotUser` — default role for any Entra user with `api.access` scope.
- `SpacebotService` — CLI client-credentials principal.
- `LegacyStatic` — static `auth_token` branch. Backward-compat; full access.
- `System` — Cortex-initiated actions.

## Matrix

| Resource | Action | Admin | User (owner) | User (team visibility) | User (not owner) | Service | Legacy | System |
|---|---|---|---|---|---|---|---|---|
| Agent | create | yes (audited) | yes | — | — | yes | yes | yes |
| Agent | read | yes (audited) | yes | yes | no (404) | yes | yes | yes |
| Agent | modify config | yes | yes | no | no | no | yes | no |
| Agent | delete | yes | yes | no | no | no | yes | no |
| Memory | read | yes (audited) | yes | yes | no (404) | no | yes | yes |
| Memory | write | yes | yes | no | no | no | yes | yes |
| Task | read | yes (audited) | yes | yes | no (404) | yes if assigned | yes | yes |
| Task | claim/complete | yes | yes | yes | no | yes if assigned | yes | yes |
| Config (providers, secrets, bindings) | read | yes | no (403) | no | no | no | yes | no |
| Config | write | yes | no | no | no | no | yes | no |
| Audit log | read | yes | no (403) | no | no | no | yes | no |
| Teams admin | all | yes | no (403) | no | no | no | no | no |
| `/api/health` | read | bypassed | bypassed | bypassed | bypassed | bypassed | bypassed | bypassed |

### Reading this table

- `no (404)`: deny by returning "not found". Do not confirm the resource exists.
- `no (403)`: role-based deny. Resource exists but the principal lacks the role.
- `yes (audited)`: admin break-glass. Emits `audit_events` with `action = 'admin_<verb>'`.

### Break-glass logging

Any admin access to another user's resource emits an audit row. These rows
roll up into the quarterly access review (Phase 10).

### Ownership transfer

Out of MVP scope. The only path in MVP is `spacebot admin claim-resource`
(Phase 9); UI-driven transfer lands post-MVP.
