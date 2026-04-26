# Postgres Backend — Phase 11 Design

Status: **DESIGN.** Not yet implemented. Phase 11 is the next initiative after the 10-phase Entra ID rollout closed (PR #120, squash `5365c90`).

> **History:** A prior design built around `sqlx::AnyPool` was archived 2026-04-25 at `docs/design-docs/archive/postgres-migration-anypool-attempt-2026-04-25.md` after first implementation attempt produced 145 chrono-related compile errors. SQLx's `Any` driver does not implement `Type`/`Decode`/`Encode` for `chrono::DateTime<Utc>` or `NaiveDateTime` ([sqlx issue #1167](https://github.com/launchbadge/sqlx/issues/1167), open since April 2021). This document supersedes that approach with enum-dispatch over native typed pools.

## Target outcome

Production Kubernetes deployments of Spacebot run against a managed PostgreSQL instance provisioned by [CloudNativePG](https://cloudnative-pg.io). Desktop and self-hosted single-tenant deployments keep SQLite. The daemon binary supports both backends via an enum that holds the native typed pool for whichever backend was selected at startup. URL-prefix selection (`sqlite:` vs `postgres:`) drives the choice at runtime; chrono types, `query_as!` macros, and `FromRow` derives all work natively because each variant uses the appropriate sqlx driver directly.

## Architectural decisions

| Decision | Choice | Why |
|----------|--------|-----|
| **Pool abstraction** | `enum DbPool { Sqlite(SqlitePool), Postgres(PgPool) }` | Native typed pools per variant. chrono works. `query_as!` works. `FromRow` works. Bypasses sqlx Issue #1167. Production pattern from atuin. |
| **Backend selection** | Runtime URL prefix (`sqlite:` vs `postgres:`/`postgresql:`) | One binary, multi-backend. Operator picks via `DATABASE_URL`. Same shape as the daemon's existing `[database]` config block. |
| **Tenancy model** | Row-level `agent_id TEXT` on per-agent tables | Postgres-native multi-tenancy. Single connection pool. Joins work. Scales linearly. (Carried over from earlier design — only the pool abstraction changed.) |
| **Backend support** | Dual: SQLite (default) + Postgres (opt-in) | Daemon supports both. SQLite remains the desktop and single-tenant default. Postgres is the K8s target. |
| **`agent_id` type** | `TEXT` | Matches today's agent-ID model. No backfill. |
| **Migration tooling** | `sqlx::migrate!` invoked per-variant from `migrations/` (SQLite) and `migrations/postgres/` (Postgres) | Compile-time-embedded migration trees. Each variant runs its own migrate! against its native pool. |
| **PR cadence** | 4-PR phased plan | PR 11.1 detailed; 11.2-11.4 outlined. Same arc as the archived design; only the per-store rewrite shape differs. |
| **Cutover** | None — greenfield | No existing K8s deployments to migrate. |
| **Pool placement** | CloudNativePG-managed (Pooler CRD if needed later) | Existing cluster pattern. |
| **Backups** | Barman → MinIO with WAL archive, 7-day retention | Mirrors `litellm-db/app/postgresql.yaml.j2` in the cluster repo. |
| **K8s sizing (v1)** | 1 instance, 50Gi storage, 4Gi memory | Spacebot is heavier than LiteLLM. |

## Backend abstraction

### `DbPool` enum

```rust
// src/db.rs
pub enum DbPool {
    Sqlite(sqlx::SqlitePool),
    Postgres(sqlx::PgPool),
}

impl DbPool {
    pub fn dialect(&self) -> Dialect {
        match self {
            DbPool::Sqlite(_) => Dialect::Sqlite,
            DbPool::Postgres(_) => Dialect::Postgres,
        }
    }

    pub async fn close(&self) {
        match self {
            DbPool::Sqlite(p) => p.close().await,
            DbPool::Postgres(p) => p.close().await,
        }
    }
}
```

### Store-level dispatch

Stores hold `Arc<DbPool>` and match on the variant inside each method. Identical SQL strings live in shared helpers; divergent SQL takes a per-variant arm.

```rust
// Hypothetical: src/memory/store.rs after PR 11.3 adoption.
pub struct MemoryStore {
    pool: Arc<DbPool>,
}

impl MemoryStore {
    pub async fn get(&self, id: &str) -> Result<Option<Memory>> {
        match &*self.pool {
            DbPool::Sqlite(p) => {
                sqlx::query_as!(Memory, "SELECT * FROM memories WHERE id = ?", id)
                    .fetch_optional(p)
                    .await
            }
            DbPool::Postgres(p) => {
                sqlx::query_as!(Memory, "SELECT * FROM memories WHERE id = $1", id)
                    .fetch_optional(p)
                    .await
            }
        }
    }
}
```

The dispatch is mechanical and testable. Each branch uses native typed pool semantics, so chrono works, `query_as!` works (with backend-specific DATABASE_URL during compilation), and `FromRow` derives compile cleanly.

### `Db` struct

```rust
pub struct Db {
    pub pool: Arc<DbPool>,
    pub lance: lancedb::Connection,
    pub redb: Arc<redb::Database>,
}
```

The `pool` field is `Arc<DbPool>` so stores can `Arc::clone` and hold their own copy. LanceDB and redb stay unchanged.

### `DialectAdapter` trait — still relevant

The `DialectAdapter` trait shipped in PR 11.1 (Tasks 11.1.2 in the prior plan) remains useful for SQL strings that differ between backends but don't warrant a full match arm — DDL fragments, `now()` vs `datetime('now')`, `BIGSERIAL` vs `INTEGER PRIMARY KEY AUTOINCREMENT`. It's a small companion abstraction, not the load-bearing dispatch layer.

```rust
pub trait DialectAdapter: Send + Sync + std::fmt::Debug {
    fn now_expr(&self) -> &'static str;
    fn autoincrement_pk(&self) -> &'static str;
    fn json_type(&self) -> &'static str;
}
```

It does NOT carry a `placeholder()` method any more. Placeholders live in the SQL string of each match arm because backend selection happens at the SQL layer, not at runtime translation.

## Why this works (and AnyPool didn't)

The chrono-on-Any gap is the practical wall. Three surface-level differences:

| Concern | AnyPool (broken) | Enum-dispatch (works) |
|---------|------------------|------------------------|
| `chrono::DateTime<Utc>` | No `Type<Any>` impl. Compile error. | Works on both `SqlitePool` and `PgPool`. |
| `query_as!` macros | Disabled when targeting Any. | Works per-arm with backend-specific DATABASE_URL. |
| `FromRow` derives | Need `FromRow<AnyRow>` which fails at chrono fields. | Need `FromRow<SqliteRow>` or `FromRow<PgRow>`, both compile. |

Atuin's [`atuin-server-database`](https://github.com/atuinsh/atuin/blob/main/crates/atuin-server-database/src/lib.rs) crate is the closest production reference: separate native impls behind a runtime-dispatched abstraction. Spacebot's `enum DbPool` is the same pattern at a smaller scale (one enum vs separate crates).

## Tenancy model — row-level `agent_id`

Unchanged from the archived design. 25 per-agent tables gain `agent_id TEXT NOT NULL` plus composite primary keys `(agent_id, id)`. New `agents` parent table in `migrations/postgres/global/`. SQLite migrations stay as-is (one DB per agent file, no `agent_id` column). The dialect divergence here is a schema-level concern handled in the per-variant migration trees.

## Migration directory layout

```
migrations/                                    # SQLite (existing, unchanged)
├── 20260211000001_memories.sql
├── ... (all 41 existing files)
└── global/
    └── ... (all 14 existing instance migrations)

migrations/postgres/                           # Postgres (new in PR 11.2/11.3)
├── 20260211000001_memories.sql
├── ... (Postgres equivalents)
└── global/
    └── ... (Postgres equivalents)
```

Each backend's `Db::connect` invocation calls `sqlx::migrate!` against the appropriate compile-time-embedded directory. No runtime path resolution needed because the variant is known at the call site:

```rust
match url_scheme {
    Scheme::Sqlite => {
        let pool = SqlitePool::connect(url).await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        DbPool::Sqlite(pool)
    }
    Scheme::Postgres => {
        let pool = PgPool::connect(url).await?;
        sqlx::migrate!("./migrations/postgres").run(&pool).await?;
        DbPool::Postgres(pool)
    }
}
```

## K8s deployment shape

Mirrors `templates/config/kubernetes/apps/database/litellm-db/app/postgresql.yaml.j2` in the cluster repo. Sized larger.

```yaml
apiVersion: postgresql.cnpg.io/v1
kind: Cluster
metadata:
  name: spacebot-db
spec:
  instances: 1
  imageName: ghcr.io/cloudnative-pg/postgresql:#{ cnpg_postgresql_version }#
  bootstrap:
    initdb:
      database: spacebot
      owner: spacebot
      secret:
        name: spacebot-db-secret
  storage:
    size: 50Gi
  postgresql:
    parameters:
      max_connections: "200"
      shared_buffers: "1GB"
      effective_cache_size: "2GB"
      work_mem: "8MB"
  resources:
    requests:
      cpu: 250m
      memory: 1Gi
    limits:
      memory: 4Gi
  monitoring:
    enablePodMonitor: true
  backup:
    barmanObjectStore:
      destinationPath: "s3://cnpg-backups/spacebot-db"
      endpointURL: "http://minio.storage.svc:9000"
      s3Credentials:
        accessKeyId:
          name: cnpg-minio-secret
          key: accessKey
        secretAccessKey:
          name: cnpg-minio-secret
          key: secretKey
      data:
        compression: gzip
        jobs: 2
        immediateCheckpoint: true
      wal:
        compression: gzip
        maxParallel: 4
      tags:
        backupRetentionPolicy: "expire"
      historyTags:
        backupRetentionPolicy: "keep"
    retentionPolicy: "7d"
```

Spacebot daemon connects via the CNPG-managed read-write service: `postgres://spacebot:$(SPACEBOT_DB_PASSWORD)@spacebot-db-rw.database.svc:5432/spacebot`.

### Schema migration on deploy

Migrations run in the daemon's startup path via `sqlx::migrate!`, same as today. CloudNativePG's `bootstrap.initdb` only creates the database + owner; the schema is the daemon's responsibility.

For multi-replica deployments, only one daemon should run migrations on startup. PR 11.4 adds a Postgres advisory lock so concurrent replica startup doesn't cause migration races.

### SOC 2 posture

The Phase 10 evidence package (PR #120) shipped against the SQLite-on-PVC posture. The Postgres deployment inherits compensating controls already in place:

- **Encryption at rest:** Talos system-disk encryption + MinIO bucket encryption for backups. Verified in the cluster repo.
- **Backup encryption:** Barman writes through KMS-backed S3; retention enforced by tags.
- **Access control:** CloudNativePG manages the `spacebot` role; the daemon connects via a Kubernetes Secret.
- **Audit:** No change. Phase 5 audit log continues to write to Postgres `audit_events` (instance-tier, ports cleanly in PR 11.2).

`docs/security/data-classification.md` gets a delta in PR 11.4 to record the Postgres posture alongside the existing SQLite-on-PVC paragraph.

## PR breakdown

### PR 11.1 — Backend abstraction (foundation)

- `enum DbPool { Sqlite, Postgres }` with `dialect()` and `close()` methods
- `DialectAdapter` trait with `SqliteDialect` + `PostgresDialect` impls
- `Db { pool: Arc<DbPool>, lance, redb }`
- `Db::connect` and `connect_instance_db` accept `Option<&str>` db_url, return `DbPool` variant matching the URL scheme
- `[database]` TOML config block (`url`, optional)
- `DbError::UnsupportedScheme` variant
- `ApiState.database_url: ArcSwap<Option<String>>` for runtime store construction
- `tests/dialect_adapter.rs` unit tests
- Stores remain on `SqlitePool` parameter type. They will be migrated in PR 11.2/11.3 as those PRs touch each subsystem. The `Db.pool` field exposes a backend-specific accessor that returns `&SqlitePool` for SQLite-mode (the path that compiles today) and a `Result` for Postgres-mode (errors with "store not yet migrated to dual-backend" until 11.2).

### PR 11.2 — Instance pool Postgres support + per-store migration tranche A

- `migrations/postgres/global/` mirrors `migrations/global/` (14 files, dialect-translated)
- Migrate the **instance-tier stores** to take `Arc<DbPool>`:
  - `TaskStore`, `WikiStore`, `ProjectStore`, `NotificationStore`, `AuditAppender`, plus instance helpers in `auth/repository.rs`, `auth/middleware.rs`, `audit/export.rs`, `config/load.rs::ensure_legacy_static_user`
- Hash-chained audit log validates against Postgres
- `connect_instance_db` actually connects to Postgres when URL is `postgres://`
- CI gains a `test-postgres-instance` job using `testcontainers` crate against `postgres:16-alpine`

### PR 11.3 — Per-agent Postgres support + per-store migration tranche B

- `migrations/postgres/` mirrors `migrations/` (41 files, dialect-translated, with `agent_id` columns)
- New `agents` parent table in `migrations/postgres/global/`
- Migrate the **per-agent stores** to take `Arc<DbPool>`:
  - `MemoryStore`, `WorkingMemoryStore`, `CronStore`, `ConversationLogger`, `ProcessRunLogger`, `ChannelStore`, `ChannelSettingsStore`, `PortalConversationStore`, `CortexLogger`, `CortexChatStore`, `AttachmentRecallTool`, plus `agent/cortex.rs::load_profile`, `agent/ingestion.rs::load_completed_chunks/delete_progress`, `agent/channel_attachments.rs` helpers
- Per-agent query rewrites: every site that touches `memories`, `channels`, etc. plumbs `agent_id` in Postgres mode
- `Db::connect` Postgres path doesn't touch the file system for the SQL tier (LanceDB + redb still per-agent file paths)
- Test fixtures use transaction-rollback isolation against a shared testcontainers Postgres instance

### PR 11.4 — K8s deployment + observability

- New cluster repo PR: `templates/config/kubernetes/apps/database/spacebot-db/` mirroring `litellm-db/`
- Spacebot Helm chart (in `deploy/helm/`) adds `DATABASE_URL` env var sourced from `spacebot-db-secret`
- Deployment gets an initContainer that waits on `spacebot-db-rw` readiness before daemon starts
- Daemon startup acquires a Postgres advisory lock before running `sqlx::migrate!` (multi-replica safety)
- `docs/security/data-classification.md` updated with Postgres-mode encryption posture
- New `docs/runbooks/spacebot-db-operations.md` covering backup verification, restore drill, role rotation
- Cluster GitOps reconciles `spacebot-db` cleanly

## Open invariants (Phase 11 register)

| Invariant | Where enforced |
|-----------|----------------|
| SQLite remains daemon default | URL-resolution dispatch in `src/db.rs` |
| `agent_id` is `TEXT` everywhere | PR 11.3 migrations |
| Migrations run in daemon startup, not in K8s Job | All PRs keep `sqlx::migrate!`; PR 11.4 adds advisory lock |
| Backups inherit cluster posture (Barman → MinIO, 7-day retention) | PR 11.4 cluster manifest |
| Audit log chain integrity preserved across migration | PR 11.2 hash-chain test against Postgres |
| LanceDB + redb backends are unaffected | Plan-wide; verified in PR 11.3 integration tests |
| Greenfield Postgres — no migration tooling for existing SQLite deployments | PR 11.1 design doc + PR 11.4 deployment runbook |
| No `sqlx::AnyPool` anywhere | This document; the enum dispatches natively |
| `chrono::DateTime<Utc>` works in `FromRow` derives | Each variant uses native typed pool — verified in PR 11.1 unit tests |

## Out of scope for Phase 11

- HA Postgres (multi-instance, sync replicas). Defer to Phase 11.5.
- PgBouncer Pooler CRD. sqlx-side pooling handles v1.
- Cross-region replication / DR. Single-cluster v1.
- Schema migration tooling other than `sqlx::migrate!`.
- Postgres for desktop. Desktop binary stays SQLite-embedded forever.

## Reference documents

- `.scratchpad/plans/postgres-backend/phase-11-postgres-backend.md` — gitignored phase plan with task-level detail
- `templates/config/kubernetes/apps/database/litellm-db/app/postgresql.yaml.j2` (in cluster repo) — CloudNativePG manifest reference pattern
- [Atuin server-database trait](https://github.com/atuinsh/atuin/blob/main/crates/atuin-server-database/src/lib.rs) — production reference for enum-dispatch dual-backend
- [SQLx Issue #1167](https://github.com/launchbadge/sqlx/issues/1167) — root cause for abandoning AnyPool
- `docs/design-docs/archive/postgres-migration-anypool-attempt-2026-04-25.md` — archived prior design
