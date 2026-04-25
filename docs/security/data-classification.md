# Data Classification

## Tiers

- **Public:** OK to share externally. Marketing content, open-source code.
- **Internal:** Spacebot team only. Design docs, runbooks.
- **Confidential:** User-visible data scoped per-principal or per-team.
- **Restricted:** Handle-with-care. API keys, audit logs, credentials.

## Mapping per table

| Table | Tier | Reasoning |
|---|---|---|
| `users` | Confidential | Real identity (display names, emails). |
| `teams` | Internal | Group metadata; no user-level data. |
| `team_memberships` | Confidential | Tells you who is in which group. |
| `service_accounts` | Confidential | Exposes automation identity. |
| `resource_ownership` | Confidential | Maps resources to owners. |
| `audit_events` | Restricted | Principal keys, IPs, actions; full forensic trail. |
| `secrets.redb` | Restricted | Encrypted credentials. |
| `memories` (per-agent) | Confidential. Restricted if containing API keys. |
| `portal_conversations` | Confidential | Private chat history. |
| `cortex_chat_messages` | Confidential | Admin chat to the cortex. |
| `tasks`, `wiki_pages`, `cron_jobs` | Confidential | User-created content. |
| `notifications` | Confidential | Per-user. |

## Handling rules

- **Restricted** data NEVER appears in logs. `src/secrets/scrub.rs::scrub_leaks` enforces this for secrets; JWT shapes were added in Phase 0.
- **Restricted** data is accessed only by admin-role principals, and every access emits `admin_read` to the audit log.
- **Confidential** data is accessed only by the owner, team members, or org members per `resource_ownership.visibility`.
- **Internal** data is free-access to authenticated principals.
- **Public** data is free-access to all.

## Encryption at rest

Spacebot stores three database backends (SQLite, LanceDB, redb) on a single data directory. The posture by deployment tier:

- **Production Kubernetes deployment** (`deploy/helm/spacebot/`). The data directory is a `PersistentVolumeClaim` on the cluster's default StorageClass. Volume-level encryption is a property of the underlying storage layer, the Talos node's system-disk encryption configuration (`.machine.systemDiskEncryption`) or the CSI-backed datastore, not of Spacebot itself. **This claim must be verified against the separate cluster repo's Talos machine config before being cited to auditors.** As of 2026-04-25, Spacebot's own repo contains no evidence of active volume encryption; the evidence lives in the cluster repo.
- **Self-hosted single-tenant deployments.** SQLite files on local disk, plaintext at the Spacebot layer. Operators are responsible for full-disk encryption (LUKS, BitLocker, FileVault) on the host. Explicitly out of Spacebot's SOC 2 scope under the shared-responsibility model.
- **Desktop app** (Tauri). SQLite files under the user's local data directory, plaintext at the Spacebot layer. End users are responsible for OS-level disk encryption.

**Application-managed secrets** (LLM API keys, Graph client_secret, Entra static `auth_token`, messaging tokens) are a separate story: encrypted at rest via `SecretsStore` (AES-256-GCM with Argon2id master key, in `secrets.redb`) regardless of deployment tier. GitOps-managed config secrets (`deploy/helm/spacebot/values.yaml`) are SOPS-encrypted with age in git and decrypted in-cluster by Flux.

**Postgres migration (roadmap).** A future migration to Postgres for the production K8s deployment is tracked in `docs/design-docs/postgres-migration.md` (stub). Not a Phase 10 deliverable; does not block SOC 2 controls because the K8s-tier compensating control is volume-level encryption at the storage layer.
