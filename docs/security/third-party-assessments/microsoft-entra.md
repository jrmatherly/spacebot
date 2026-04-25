# Third-Party Risk: Microsoft Entra ID

## Services Spacebot depends on

| Service | Purpose | Reliance |
|---|---|---|
| Microsoft Identity Platform (v2) | JWT issuance, JWKS | Hard, runtime |
| Microsoft Graph API | Group-membership overage resolution | Soft, fail-closed to empty groups |
| Conditional Access | MFA, device compliance, IP restrictions | Soft, policy-driven, tenant admin owns |
| Entra audit logs | Cross-reference for security investigations | Soft, read-only external |

## Risk assessment

- **Microsoft's SOC 2 Type II:** reliance accepted. Attestation retrieved annually by the account's Microsoft representative.
- **Microsoft's ISO 27001:** additional reliance accepted.
- **Outage scenarios:**
  - Entra outage: all new sign-ins fail; existing tokens remain valid until `exp`.
  - Graph outage: team-scope authz denies until recovery.

## Compensating controls

- JWT validation is local via JWKS cache, so the daemon survives brief Entra hiccups.
- The Phase 10 jwks.rs fix sets `refresh_interval = 0`, so unknown-kid tokens trigger an immediate refetch when Entra rotates keys.
- The legacy `auth_token` path remains available as an emergency admin route via config.
- The audit log is append-only and verifiable without Entra dependency.

## Evidence links

- Microsoft's public attestations: `https://learn.microsoft.com/compliance/regulatory/offering-soc`.
- Internal: annual Microsoft account-rep update is logged in the compliance tracker.

## Cross-references

- `docs/security/data-classification.md`: per-table tier mapping and encryption-at-rest posture.
- `docs/runbooks/entra-auth-incidents.md`: response playbooks tied to Microsoft-side failures.
