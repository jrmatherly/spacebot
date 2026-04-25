# Postgres Backend — Phase 11 Design

Status: **DESIGN.** Not yet implemented. Phase 11 is the next initiative after the 10-phase Entra ID rollout closed (PR #120, squash `5365c90`).

## Target outcome

Production Kubernetes deployments of Spacebot run against a managed PostgreSQL instance provisioned by [CloudNativePG](https://cloudnative-pg.io) instead of SQLite-on-PVC. Desktop and self-hosted single-tenant deployments keep SQLite. The daemon binary supports both backends through a `sqlx::AnyPool` abstraction with runtime backend selection from the connection-string scheme.

The Spacebot codebase already converged on conditions that make this migration tractable: zero `sqlx::query!` compile-time macros (verified against `main` at the start of Phase 11), all queries use runtime `sqlx::query()` with stringly-typed SQL. The structural work concentrates at two pool-construction seams (`src/db.rs::Db::connect` and `src/db.rs::connect_instance_db`), not at every query site.

## Architectural decisions

| Decision | Choice | Why |
|----------|--------|-----|
| **Tenancy model** | Row-level `agent_id` column on every per-agent table | Postgres-native multi-tenancy. Single connection pool. Joins work. Scales linearly. Schema-per-agent and database-per-agent both hit operational walls at scale. |
| **Backend abstraction** | `sqlx::AnyPool` + `DialectAdapter` trait | Runtime backend selection from connection-string scheme. One binary serves both backends. Dialect-specific shims (datetime/now, AUTOINCREMENT/BIGSERIAL) live behind the trait. |
| **Backend support** | Dual: SQLite (default) + Postgres (opt-in) | Daemon supports both. SQLite remains the desktop and single-tenant default. Postgres is the multi-tenant K8s target. Selection by `DATABASE_URL` scheme. |
| **`agent_id` type** | `TEXT` | Matches today's agent-ID model: string IDs flow through file paths, CLI args, config TOML. Avoids backfill conversion of existing TEXT IDs that may not be UUIDs. |
| **Migration tooling** | `sqlx::migrate!` with parallel `migrations/postgres/` tree | Single tool for both backends. Same numeric prefix per migration, different dialect-adapted SQL. Backend selection at runtime picks which directory to run. |
| **PR cadence** | 4-PR phased plan | Each PR lands independently and passes `just gate-pr`. PR 11.1 is detailed; 11.2-11.4 expand just before they're worked. |
| **Cutover** | None — greenfield | No existing K8s deployments to migrate. Postgres mode applies to new deployments. |
| **Pool placement** | CloudNativePG-managed; add Pooler CRD only if connection pressure warrants | Existing cluster pattern. Application-side `sqlx::PgPool` handles per-pod pooling. |
| **Backups** | Barman → MinIO with WAL archive, 7-day retention | Mirrors `litellm-db/app/postgresql.yaml.j2` in the cluster repo. Existing operational pattern. |
| **K8s sizing (v1)** | 1 instance, 50Gi storage, 4Gi memory | Spacebot's audit log + per-agent traffic + memory metadata is heavier than LiteLLM's reference shape. Single instance for v1; HA replicas added when production traffic warrants. |

## Why this is tractable

**No `sqlx::query!` compile-time macros.** A 2026-04-22 inventory found zero. Re-verified at the start of Phase 11. Every query is runtime-string `sqlx::query()`. The original 4-8 week estimate (per the superseded stub) assumed per-feature offline caches keyed to a SQLite schema. With runtime queries, dialect adaptation is concentrated at the SQL string layer, not at the type system.

**Two narrow pool-construction seams.** `src/db.rs::Db::connect` builds the per-agent pool; `src/db.rs::connect_instance_db` builds the instance pool. Both are called from a small set of sites (~4 in `src/main.rs` and `src/api/agents.rs`). The backend abstraction lives at these seams, not inside every handler.

**LanceDB and redb are unaffected.** Vector embeddings stay in LanceDB. Per-agent key-value config stays in redb. Only the relational SQLite tier moves.

## Backend abstraction

### The seam

```rust
// src/db.rs (today)
pub struct Db {
    pub sqlite: SqlitePool,
    pub lance: lancedb::Connection,
    pub redb: Arc<redb::Database>,
}
```

After PR 11.1:

```rust
// src/db.rs (after Phase 11.1)
pub struct Db {
    pub sql: AnyPool,
    pub dialect: Dialect,
    pub lance: lancedb::Connection,
    pub redb: Arc<redb::Database>,
}

pub enum Dialect {
    Sqlite,
    Postgres,
}

pub trait DialectAdapter: Send + Sync {
    fn now_expr(&self) -> &'static str;            // "datetime('now')" vs "now()"
    fn autoincrement_pk(&self) -> &'static str;    // "INTEGER PRIMARY KEY AUTOINCREMENT" vs "BIGSERIAL PRIMARY KEY"
    fn json_type(&self) -> &'static str;           // "TEXT" vs "JSONB"
    fn upsert_clause(&self) -> &'static str;       // "ON CONFLICT(...) DO UPDATE SET ..." (both support, but Postgres EXCLUDED syntax differs)
    fn placeholder(&self, n: usize) -> String;     // "?" vs "$1"
}
```

The placeholder difference is the most invasive divergence. SQLx's `AnyPool` translates `?` to `$1` automatically *when binding through the Any driver*, so this is a soft constraint. Writing `$1` natively in Postgres-only migrations is clearer.

### How queries pick a dialect

Two patterns, picked per call site:

**Pattern A (preferred):** SQL strings that work on both dialects unchanged. Most simple SELECT/INSERT/UPDATE/DELETE statements fall here. SQLx's Any driver handles binding, type coercion, and result decoding.

**Pattern B (when divergent):** A small `format!` with `dialect.now_expr()` or similar. Used in the ~20 sites that hit `datetime()` / `strftime()` / `json_extract()`.

Inventory of dialect-divergent SQL idioms (from `grep` against current `migrations/`):

- `INTEGER PRIMARY KEY AUTOINCREMENT` (8 sites in 8 migration files)
- `datetime('now')` and `datetime(...)` defaults (12 sites)
- `json_extract(...)` (3 sites)
- `WITHOUT ROWID` (0 sites currently)
- `strftime(...)` (0 sites currently)

Total: ~23 SQL string sites need dialect adaptation. The query-site count (273 runtime queries) is mostly orthogonal to the dialect divergence. Most queries just SELECT/INSERT into tables that are dialect-clean.

### Connection-string-driven selection

```rust
let pool = AnyPoolOptions::new()
    .max_connections(cfg.db.max_connections)
    .connect(&database_url)
    .await?;

let dialect = match Url::parse(&database_url)?.scheme() {
    "sqlite" => Dialect::Sqlite,
    "postgres" | "postgresql" => Dialect::Postgres,
    other => bail!("unsupported db scheme: {other}"),
};
```

`DATABASE_URL` env var or `[database]` TOML block becomes the operator-facing knob. Default for desktop: `sqlite://~/.spacebot/...`. Default for K8s: `postgres://spacebot:...@spacebot-db-rw.database.svc:5432/spacebot`.

## Tenancy model — row-level `agent_id`

### Per-agent tables today (SQLite)

25 tables under `migrations/`:

```
agent_profile, associations, branch_runs, channel_settings, channels,
conversation_messages, cortex_chat_messages, cortex_events,
cron_executions, cron_jobs, ingestion_files, ingestion_progress,
memories, project_repos, project_worktrees, projects, projects_new,
saved_attachments, tasks, token_usage, webchat_conversations,
worker_runs, working_memory_daily_summaries, working_memory_events,
working_memory_intraday_syntheses
```

Each lives in its own `agent.db` file under `~/.spacebot/agents/{id}/data/`.

### Per-agent tables after PR 11.3 (Postgres)

Same 25 tables, but under a single Postgres database. Each gains:

```sql
agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE
```

Composite primary keys become `(agent_id, id)`. Per-table indexes get `agent_id` as a leading column where the access pattern is "fetch agent X's rows."

The `agents` table is new in PR 11.3 as the parent table for the FK. In SQLite mode, this table is unused (per-agent DBs don't need a parent registry). In Postgres mode, it's populated by the agent factory at agent creation.

### Instance-wide tables (no change to tenancy model)

16 tables under `migrations/global/`:

```
audit_events, audit_export_state, notifications, project_repos,
project_worktrees, projects, resource_ownership, service_accounts,
spacedrive_pairing, task_number_seq, tasks, team_memberships, teams,
users, wiki_page_versions, wiki_pages
```

These are already shared across agents and need no `agent_id` column. They migrate in PR 11.2 with no schema-shape change.

(Note: `projects`, `tasks`, `project_repos`, `project_worktrees` appear in both tiers. The instance-wide versions are the active path; the per-agent versions are legacy and may get dropped in a separate cleanup. Phase 11 ports both for fidelity.)

### Query rewrite cardinality

Per-agent table queries currently look like:

```rust
sqlx::query("SELECT * FROM memories WHERE id = ?").bind(id)
```

After PR 11.3:

```rust
sqlx::query("SELECT * FROM memories WHERE agent_id = ? AND id = ?")
    .bind(&agent_id)
    .bind(id)
```

The agent_id is plumbed from `Db` (which carries it as a new field in Postgres mode) or implicitly from the per-agent SQLite file's identity (in SQLite mode the column doesn't exist; the SQL needs dialect-aware handling).

A cleaner pattern: every per-agent query takes the agent_id explicitly, and in SQLite mode the column simply isn't there (the query string is dialect-conditional). The downside is per-call-site dialect awareness; the upside is no schema divergence between modes.

This is the largest concentration of edit volume in Phase 11. Estimated: ~100-150 query-site rewrites across `src/memory/`, `src/agent/`, `src/conversation/`, `src/tasks/` (per-agent slice), `src/cron.rs`, and similar.

## Migration directory layout

```
migrations/
├── 20260211000001_memories.sql        # SQLite (existing, unchanged)
├── 20260211000002_conversations.sql
├── ... (all 41 existing files unchanged)
├── global/
│   └── ... (all 14 existing instance migrations unchanged)
└── postgres/
    ├── 20260211000001_memories.sql    # Postgres equivalent (new in PR 11.2/11.3)
    ├── 20260211000002_conversations.sql
    ├── ...
    └── global/
        └── ...
```

Numeric prefixes match across SQLite and Postgres siblings. The runtime picks the directory at `Db::connect` time based on backend.

For the per-agent tier, Postgres migrations create the unified `agents` parent table once (in `migrations/postgres/global/`) and add `agent_id` columns + composite PKs in each per-agent migration.

## K8s deployment shape

Mirrors `litellm-db/app/postgresql.yaml.j2` in the cluster repo. Sized larger.

```yaml
# templates/config/kubernetes/apps/database/spacebot-db/app/postgresql.yaml.j2
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

For multi-replica deployments, only one daemon should run migrations on startup. PR 11.4 adds a startup leader-election guard (Postgres advisory lock) so concurrent replica startup doesn't cause migration races.

### SOC 2 posture

The Phase 10 evidence package (PR #120) shipped against the SQLite-on-PVC posture. The Postgres deployment inherits compensating controls already in place:

- **Encryption at rest:** Talos system-disk encryption at the storage layer + MinIO bucket encryption for backups. Verified in the cluster repo, NOT a Spacebot-side claim.
- **Backup encryption:** Barman writes through KMS-backed S3 endpoint; retention policy enforced by tags.
- **Access control:** CloudNativePG manages the `spacebot` role; the daemon connects via a Kubernetes Secret. Role-grant management is operator runbook.
- **Audit:** No change. Phase 5 audit log continues to write to Postgres `audit_events` (instance-tier, ports cleanly in PR 11.2).

`docs/security/data-classification.md` gets a delta in PR 11.4 to record the Postgres posture alongside the existing SQLite-on-PVC paragraph.

## Acceptance criteria per PR

### PR 11.1 — Backend abstraction

- `src/db.rs` carries `Db { sql: AnyPool, dialect: Dialect, ... }`
- `DialectAdapter` trait + `SqliteDialect` / `PostgresDialect` impls
- All existing tests pass with no behavior change (SQLite path unchanged)
- `cargo check --features postgres` compiles (no Postgres pool construction yet)
- `just gate-pr` green

### PR 11.2 — Instance pool Postgres support

- `migrations/postgres/global/` mirrors `migrations/global/` semantically
- `connect_instance_db` accepts both `sqlite://` and `postgres://` URLs
- Dual-`DATABASE_URL` CI matrix runs the instance-tier integration tests against both backends
- `just gate-pr` green; new CI job `test-postgres-instance` green

### PR 11.3 — Per-agent Postgres support (largest PR)

- `migrations/postgres/` mirrors `migrations/` with `agent_id` columns added and composite PKs
- `agents` parent table introduced in `migrations/postgres/global/`
- Per-agent query sites rewritten to plumb `agent_id` (or use a `with_agent(id)` helper)
- `Db::connect` Postgres path attaches an agent without touching the file system
- Per-agent integration tests pass against both backends
- `just gate-pr` green

### PR 11.4 — K8s deployment + observability

- Cluster repo PR adds `templates/config/kubernetes/apps/database/spacebot-db/`
- Spacebot Helm chart adds `DATABASE_URL` env var + Secret reference + initContainer to wait on `spacebot-db-rw` readiness
- Migration leader-election via Postgres advisory lock
- `docs/security/data-classification.md` updated with Postgres-mode encryption-at-rest paragraph
- `docs/runbooks/spacebot-db-operations.md` (new) covers backup verification, restore drill, role rotation
- `just gate-pr` green; cluster GitOps reconciles `spacebot-db` cleanly

## Open invariants (Phase 11 register)

These carry forward from the Entra rollout discipline:

| Invariant | Where enforced |
|-----------|----------------|
| Greenfield Postgres — no migration tooling for existing SQLite deployments | PR 11.1 design doc + PR 11.4 deployment runbook |
| SQLite remains the daemon default; Postgres is opt-in via `DATABASE_URL` scheme | PR 11.1 connection-selection logic |
| `agent_id` is `TEXT` everywhere | PR 11.3 migrations |
| Migrations run in daemon startup, not in K8s Job | PR 11.1 keeps `sqlx::migrate!`; PR 11.4 adds advisory lock |
| Backups inherit cluster posture (Barman → MinIO, 7-day retention) | PR 11.4 cluster manifest |
| Audit log chain integrity preserved across the migration | PR 11.2 hash-chain test against Postgres |
| LanceDB + redb backends are unaffected | Plan-wide; verified in PR 11.3 integration tests |

## Out of scope for Phase 11

- **HA Postgres (multi-instance, sync replicas).** Defer to a Phase-11.5 follow-up once production traffic warrants. v1 ships single-instance.
- **PgBouncer Pooler CRD.** sqlx-side pooling handles v1 connection counts. Add the Pooler CRD when total connections approach `max_connections=200`.
- **Per-tenant connection limits.** No tenancy quotas at the Postgres layer in v1; rely on application-layer rate limits.
- **Cross-region replication or DR.** Single-cluster v1.
- **Schema migration tooling other than sqlx::migrate!.** No refinery, no flyway, no separate migration job.
- **Postgres for desktop.** Desktop binary stays SQLite-embedded forever.

## Reference documents

- `.scratchpad/plans/postgres-backend/phase-11-postgres-backend.md` — gitignored phase plan with task-level detail (PR 11.1 fully expanded; 11.2-11.4 outlined)
- `templates/config/kubernetes/apps/database/litellm-db/app/postgresql.yaml.j2` (in cluster repo) — CloudNativePG manifest reference pattern
- Phase 10 plan (gitignored) `Task 10.8b` — original superseded-stub context and the 2026-04-22 sqlx::query! inventory finding
