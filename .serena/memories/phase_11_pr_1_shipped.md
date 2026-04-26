# Phase 11 PR 11.1 — Postgres Backend Foundation (shipped 2026-04-26)

Squash commit `7eba8cd` on main. PR #121, 14 files, +944/-100, 952 lib tests.

## What landed

The foundation for dual SQLite + Postgres backend support. Per-store dispatch and Postgres migration trees are deferred to PR 11.2/11.3.

### New types in `src/db.rs`

- **`enum DbPool { Sqlite(SqlitePool), Postgres(PgPool) }`** — load-bearing dispatch over native typed sqlx pools. Bypasses sqlx Issue #1167 (chrono not impl'd through Any). Exposes `dialect()`, `adapter()`, `close()`, `as_sqlite()` accessors.
- **`enum DatabaseUrl { Sqlite(SqliteUrl), Postgres(PostgresUrl) }`** — `#[non_exhaustive]` newtype with **private payload structs**. `SqliteUrl`/`PostgresUrl` wrap `String` in private fields so the only constructor is `DatabaseUrl::from_str`. Variant tag and content cannot disagree. Manual `Debug` impl uses `redact_url()` to mask credentials. **No `Serialize` impl by design** — typed URL must not flow into HTTP responses or JSON dumps.
- **`enum Dialect { Sqlite, Postgres }`** — `#[non_exhaustive]` Copy tag.
- **`enum MigrationsTree { PerAgent, Instance }`** — `#[non_exhaustive]` directory selector. PerAgent → `migrations/`, Instance → `migrations/global/`. Postgres-side variants land in PR 11.2/11.3.
- **`Db { pool: Arc<DbPool>, lance, redb }`** — replaces previous `Db { sqlite: SqlitePool, ... }`. `Db::sqlite_pool()` accessor returns `Result<&SqlitePool>` (Err on Postgres variant). Used as transitional bridge while stores still take `&SqlitePool`.

### New `src/dialect.rs` module

- `DialectAdapter` trait with `now_expr()`, `autoincrement_pk()`, `json_type()` returning `&'static str`.
- `SqliteDialect` and `PostgresDialect` zero-state impls.
- Per-query placeholder syntax (`?` vs `$1`), JSON operators, `RETURNING` clauses live inside per-variant store dispatch in PR 11.2/11.3, **NOT in this trait**. The trait is a small companion abstraction for DDL fragments only.

### Config wiring

- `[database]` TOML block via `TomlDatabaseConfig { url: Option<DatabaseUrl> }` in `src/config/toml_schema.rs` and `DatabaseConfig` in `src/config/types.rs`. URL classification runs at TOML deserialize time (custom `Deserialize` impl forwards to `FromStr`), so operator typos surface during startup not at first connect.
- `ApiState.database_url: ArcSwap<Option<DatabaseUrl>>` populated by `set_database_url(...)` at daemon startup. **STARTUP ONLY** — the `pub` setter is documented for startup use, not handler use. Hot-reload not supported in PR 11.1.

### Migration runner

Switched from `sqlx::migrate!(literal)` macro to `sqlx::migrate::Migrator::new(Path)` runtime-loaded migrator. Trade: migrations load from disk at startup rather than embed in the binary. Right call for daemon shipping with adjacent `migrations/` directory; if binary distribution becomes a goal, switch back via per-arm `migrate!()` invocation.

### Postgres-URL fail-fast

`open_pool_and_migrate` Postgres arm returns `DbError::Other(...)` with a message pointing at PR 11.2/11.3. PR 11.1 ships zero `migrations/postgres/` files — the path is genuinely unblockable until PR 11.2 lands the instance-tier Postgres migration tree.

## Bridge invariant: `AgentDeps.sqlite_pool` (lib.rs:425)

Survives PR 11.1 unchanged as `sqlx::SqlitePool` (not `Arc<DbPool>`). The construction site at `main.rs:3288` was swept from `db.sqlite.clone()` to `db.sqlite_pool().expect(...).clone()` then to `?` propagation in R4. The 25+ transitive consumers across `src/agent/{channel,branch,worker,ingestion}.rs`, `src/tools/spawn_worker.rs`, `src/tools.rs`, `src/main.rs` continue receiving real `SqlitePool` clones. Sqlx pool clones share an internal `Arc`, so the audit-log singleton invariant (A-13) is undisturbed.

PR 11.3 will migrate `AgentDeps` to `Arc<DbPool>`.

## Security additions

- **`redact_url(&str) -> String`** at `src/db.rs` strips `user:pass@` from URLs before embedding in error messages and panic strings. Limitations: doesn't handle query-string credentials (`?password=...`) or URL-encoded user:pass. Sufficient for PR 11.1's two callers.
- **Manual `Debug` impl on `DatabaseUrl`/`SqliteUrl`/`PostgresUrl`** — `tracing::debug!(?url, ...)` is now safe. Was a real risk: derived `Debug` would have emitted `Postgres("postgres://user:secret@host/db")` verbatim.
- **`DbError::UnsupportedScheme(String)` variant** uses redacted form when constructed.

## Process notes

Two-round multi-agent review pattern caught issues 17+ commits of self-review didn't:

- **1st pass (after initial 10 commits)**: 5 important findings (credential leak in error messages, .expect() ambiguity + decay, missing DatabaseUrl newtype, untested legacy renames, untested close paths). Addressed across R1–R5 (6 remediation commits).
- **2nd pass (after R1-R5)**: 2 important findings introduced by R3's newtype work (Serialize derive leaks credentials via JSON, Debug derive leaks via tracing). Plus 4 type-design improvements (#[non_exhaustive], private newtype payloads, redundant Deserialize impl, lone .expect() restructure). Addressed in R6 single commit.

Pattern worth repeating on Phase 11.2/11.3 PRs: the second-pass review specifically catches regressions introduced by remediation work itself.

## Test count delta

- 952 lib tests (up from 936 pre-PR)
- 16 new tests added across remediation:
  - 4 `database_url_*` parse + dialect + redaction
  - 1 `database_url_deserializes_via_serde`
  - 8 legacy-rename branch coverage (4 per resolve_*_url helper)
  - 1 `connect_instance_db_with_sqlite_url_runs_global_migrations`
  - 2 close-path coverage (`dbpool_sqlite_close`, `db_close`)
  - 3 redaction tests (`redact_url_*`, `database_url_redacts_credentials_in_unsupported_scheme_error`)
  - 1 `rejects_unknown_scheme_at_config_load_not_at_connect` in config::load::tests
- 6 new `tests/dialect_adapter.rs` integration tests (one per trait method × 2 impls)

## Out of scope (PR 11.2/11.3/11.4)

- Per-store dispatch (NotificationStore, TaskStore, MemoryStore, WikiStore, etc.) — PR 11.2/11.3
- `migrations/postgres/` tree — PR 11.2 (instance) + PR 11.3 (per-agent)
- testcontainers integration tests — PR 11.2
- `AgentDeps.sqlite_pool` → `Arc<DbPool>` migration — PR 11.3
- K8s manifest + CloudNativePG cluster + Helm wiring — PR 11.4
- Postgres advisory-lock leader election for migrations — PR 11.4

## Reference

- Public design doc: `docs/design-docs/postgres-migration.md` (committed `0fd21aa`)
- Archived AnyPool design (do NOT use for planning): `docs/design-docs/archive/postgres-migration-anypool-attempt-2026-04-25.md` (committed `ff9bcf4`)
