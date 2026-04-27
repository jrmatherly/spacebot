# Phase 11 PR 11.2 — Instance-tier Postgres dispatch (shipped 2026-04-27)

> **Cross-session retention anchor:**
> - **PR #136 merged at squash commit `32d5858` (2026-04-27 17:55 UTC).** Branch `feat/phase-11-pr-2-instance-postgres` is fully cleaned up. No follow-up R3+ remediation pending.
> - **PR 11.1 merged at squash commit `7eba8cd`** (shipped in v0.6.0).
> - **The daemon still defaults to SQLite.** Postgres is opt-in via `[database] url = "postgres://..."` in config.toml. Instance-tier dispatch now works on both backends; per-agent dispatch lands in PR 11.3.
> - **Authoritative reference:** `docs/design-docs/postgres-migration.md`. PR 11.2 SHIPPED block at lines 252-269.
> - **Tracking issue for deferred follow-ups:** [#138](https://github.com/jrmatherly/spacebot/issues/138).

## What landed in PR 11.2

### Migrations
- `migrations/postgres/global/` — 14 files, dialect-translated. FTS5 → tsvector STORED column + GIN index. INSERT OR IGNORE → ON CONFLICT (kind, related_entity_type, related_entity_id) WHERE dismissed_at IS NULL DO NOTHING. randomblob/hex → gen_random_uuid()::text. INTEGER bytes/seq/version → BIGINT.

### Stores migrated to `Arc<DbPool>` dispatch
- `TaskStore` (CreateOutcome enum bridges native error codes 2067 vs 23505)
- `WikiStore` (FTS5 → tsvector @@ websearch_to_tsquery; per-backend row readers)
- `ProjectStore` (placeholder + DATETIME→String vs TIMESTAMPTZ→DateTime<Utc>)
- `NotificationStore` (Pattern C: INSERT OR IGNORE vs ON CONFLICT (...) WHERE ... DO NOTHING)
- `AuditAppender` (per-backend transaction body; A-13 atomic-ordering preserved)

### Auth + config helpers migrated to `&DbPool`
- `auth/repository.rs` (12 fns; PG_USERS_COLUMNS / PG_TEAMS_COLUMNS / PG_RESOURCE_OWNERSHIP_COLUMNS as plain column-list constants — earlier `to_char(... AT TIME ZONE 'UTC', ...)` projection was schema/dialect drift, fixed in R2)
- `auth/middleware.rs` (`sync_groups_for_principal`, `sync_user_photo_for_principal`)
- `auth/policy.rs` (5 fns)
- `config/load.rs` (`ensure_legacy_static_user`, `reconcile_toml_agents_with_ownership`)
- `audit::export::export_audit`

### State + handlers
- `ApiState.instance_pool` widened from `ArcSwap<Option<SqlitePool>>` to `ArcSwap<Option<Arc<DbPool>>>`.
- A-13 atomic-ordering invariant preserved: `set_instance_pool` stores AuditAppender BEFORE `instance_pool` so racing observers always see audit=Some when instance_pool=Some.
- ~30 handler call sites swept across `src/api/{agents,cron,admin_*,me,resources,...}.rs`.
- `src/main.rs` drops `.as_sqlite()?` PR 11.1 transitional bridge.
- Legacy migrators (`tasks::migration::migrate_legacy_tasks`, `projects::migration::migrate_legacy_projects`) take `&DbPool`, no-op on Postgres arm.

### Tests
- `tests/instance_postgres.rs` (T16+T17): testcontainers + `postgres:18-alpine` (matches CNPG production target on Talos). 8 tests covering migrations smoke, audit hash chain, TaskStore CRUD, WikiStore tsvector search, NotificationStore ON CONFLICT, ProjectStore CRUD, auth_repository upsert (added in R2), audit_export cursor (added in R2).
- `AuditAppender::new_for_tests_pg(Arc<DbPool>)` test-only constructor (sibling to existing `new_for_tests(SqlitePool)`).
- 26 existing integration test files swept (T15b) to wrap `SqlitePool` into `Arc<DbPool::Sqlite(...))` at fixture sites.

### CI
- `test-postgres-instance` job in `.github/workflows/ci.yml`. Runs `cargo test --test instance_postgres` against testcontainers `postgres:18-alpine`. SHA-pinned actions matching existing `test` job. Separate cache slot `ci-rust-postgres`.
- `persist-credentials: false` on the new checkout step (closes zizmor #135).

## Hard rules (inherited from PR 11.1, preserved through PR 11.2)

1. **A-13 audit appender singleton.** `AuditAppender::new` stays `pub(crate)`. Test-only `new_for_tests(SqlitePool)` and `new_for_tests_pg(Arc<DbPool>)` are `#[doc(hidden)] pub`.
2. **Atomic-ordering invariant in `set_instance_pool`.** AuditAppender stored before instance_pool — any thread reading `instance_pool=Some` is guaranteed to see `audit=Some`.
3. **Postgres timestamp columns are TEXT** (not TIMESTAMPTZ). Both backends store the canonical ISO-8601 form. Read-side projections are plain column lists — NEVER `to_char(text_col AT TIME ZONE 'UTC', ...)` (was the schema/dialect drift R2 caught).
4. **No in-store SQL macros for new code.** Phase 11.2 stores use `sqlx::query(...)` (runtime-string) or `sqlx::query_as(...)` (FromRow). The `sqlx::migrate!()` macro stays in existing `#[cfg(test)]` modules unchanged.
5. **`testcontainers-modules` 0.15 default tag is `11-alpine`.** Always pin via `Postgres::default().with_tag("18-alpine")` — Postgres 11 fails the `GENERATED ALWAYS AS (...) STORED` syntax (PG 12+) used by the wiki migration.

## Out of scope (deferred to PR 11.3 — tracked in #138)

- Per-agent stores: `MemoryStore`, `WorkingMemoryStore`, `CronStore`, `ConversationLogger`, `ProcessRunLogger`, `ChannelStore`, `ChannelSettingsStore`, `PortalConversationStore`, `CortexLogger`, `CortexChatStore`, `AttachmentRecallTool`
- Per-agent helpers: `agent/cortex.rs::load_profile`, `agent/ingestion.rs::load_completed_chunks/delete_progress`, `agent/channel_attachments.rs`
- `migrations/postgres/` per-agent tree (41 files)
- `agents` parent table in `migrations/postgres/global/`
- `AgentDeps.sqlite_pool` → `Arc<DbPool>`
- 11 untested Postgres dispatch arms in `auth/repository.rs` + `audit/export.rs` + `admin.rs::sweep_orphans` + `auth/middleware.rs::sync_*` + `auth/policy.rs`
- Audit-chain mutation detection + cross-backend hash determinism tests
- Dispatch boilerplate refactor (the helper should cover per-agent stores too)

## Out of scope (deferred to PR 11.4)

- K8s manifests + Helm wiring + CloudNativePG cluster manifest (PG 18 on Talos)
- Postgres advisory-lock leader election for migrations (multi-replica safety)
- `docs/security/data-classification.md` Postgres-mode encryption posture delta
- `docs/runbooks/spacebot-db-operations.md` (backup verification, restore drill, role rotation)

## Reference

- Public design doc: `docs/design-docs/postgres-migration.md` (PR 11.2 SHIPPED block at lines 252-269)
- Squash merge commit: `32d5858` on main
- Original feature branch: `feat/phase-11-pr-2-instance-postgres` (deleted from origin)
- Tracking issue for deferred R2 follow-ups: [#138](https://github.com/jrmatherly/spacebot/issues/138)
- Closes [zizmor #135](https://github.com/jrmatherly/spacebot/security/code-scanning/135)
