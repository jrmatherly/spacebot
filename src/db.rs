//! Database connection management and migrations.
//!
//! Phase 11 introduces a Postgres backend alongside SQLite via the
//! `DbPool` enum. Each variant holds a native typed sqlx pool, so
//! chrono types, `query_as!` macros, and `FromRow` derives all work
//! naturally per variant because there's no `Any` driver in the
//! dispatch path.
//! Backend selection happens at runtime from the `DATABASE_URL` scheme:
//! `sqlite:` (or unset) routes to SQLite; `postgres:`/`postgresql:`
//! routes to Postgres. See `docs/design-docs/postgres-migration.md`.

use crate::dialect::{DialectAdapter, PostgresDialect, SqliteDialect};
use crate::error::{DbError, Result};

use anyhow::Context as _;
use serde::Deserialize;
use sqlx::{PgPool, SqlitePool};

use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

/// Backend dialect selected at connection time. Drives migration directory
/// selection and accompanies the pool for handlers that need to branch on
/// backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Dialect {
    Sqlite,
    Postgres,
}

/// SQLite connection string. Constructed only by `DatabaseUrl::from_str`,
/// which guarantees the wrapped string starts with `sqlite:`.
#[derive(Clone, PartialEq, Eq)]
pub struct SqliteUrl(String);

impl SqliteUrl {
    /// Borrow the underlying connection string for sqlx.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SqliteUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SqliteUrl")
            .field(&redact_url(&self.0))
            .finish()
    }
}

/// Postgres connection string. Constructed only by `DatabaseUrl::from_str`,
/// which guarantees the wrapped string starts with `postgres:` or
/// `postgresql:`. Debug formatting redacts `user:pass@` credentials so
/// stray `tracing::debug!(?url, ...)` calls cannot leak operator secrets.
#[derive(Clone, PartialEq, Eq)]
pub struct PostgresUrl(String);

impl PostgresUrl {
    /// Borrow the underlying connection string for sqlx.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for PostgresUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PostgresUrl")
            .field(&redact_url(&self.0))
            .finish()
    }
}

/// Validated database connection URL with backend dialect encoded in the
/// type. Constructed via `FromStr` (or `serde::Deserialize`), which
/// classifies the scheme prefix at parse time. Operator typos surface
/// during TOML deserialization, not at connect time.
///
/// Use `dialect()` to extract the matching `Dialect` tag, `as_str()` to
/// borrow the inner connection string for sqlx. Variant payloads
/// (`SqliteUrl`, `PostgresUrl`) wrap the raw string in private fields so
/// downstream code cannot construct a variant whose tag and content
/// disagree (e.g., `Sqlite` holding a `postgres://` string).
///
/// `Debug` formatting redacts `user:pass@` credentials per variant.
/// `Serialize` is intentionally NOT implemented: the typed URL must not
/// flow into HTTP responses or JSON dumps where credentials would leak.
#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(try_from = "String")]
#[non_exhaustive]
pub enum DatabaseUrl {
    Sqlite(SqliteUrl),
    Postgres(PostgresUrl),
}

impl DatabaseUrl {
    /// Backend dialect tag.
    pub fn dialect(&self) -> Dialect {
        match self {
            DatabaseUrl::Sqlite(_) => Dialect::Sqlite,
            DatabaseUrl::Postgres(_) => Dialect::Postgres,
        }
    }

    /// Borrow the underlying connection string for sqlx.
    pub fn as_str(&self) -> &str {
        match self {
            DatabaseUrl::Sqlite(u) => u.as_str(),
            DatabaseUrl::Postgres(u) => u.as_str(),
        }
    }
}

impl std::fmt::Debug for DatabaseUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseUrl::Sqlite(u) => f.debug_tuple("Sqlite").field(u).finish(),
            DatabaseUrl::Postgres(u) => f.debug_tuple("Postgres").field(u).finish(),
        }
    }
}

impl FromStr for DatabaseUrl {
    type Err = DbError;

    fn from_str(url: &str) -> std::result::Result<Self, Self::Err> {
        if url.starts_with("sqlite:") {
            Ok(DatabaseUrl::Sqlite(SqliteUrl(url.to_string())))
        } else if url.starts_with("postgres:") || url.starts_with("postgresql:") {
            Ok(DatabaseUrl::Postgres(PostgresUrl(url.to_string())))
        } else {
            Err(DbError::UnsupportedScheme(redact_url(url)))
        }
    }
}

impl TryFrom<String> for DatabaseUrl {
    type Error = DbError;

    fn try_from(url: String) -> std::result::Result<Self, Self::Error> {
        url.parse()
    }
}

/// Backend-typed connection pool. Each variant holds the native sqlx pool
/// so chrono types, query_as! macros, and FromRow derives work per variant.
///
/// `Debug` is derived (both `SqlitePool` and `PgPool` already implement it)
/// so any store struct holding `Arc<DbPool>` can keep its own derived
/// `Debug`. The pool's Debug output may include connection metadata; do not
/// log it at info level on hot paths.
#[derive(Debug)]
pub enum DbPool {
    Sqlite(SqlitePool),
    Postgres(PgPool),
}

impl DbPool {
    /// Backend dialect tag.
    pub fn dialect(&self) -> Dialect {
        match self {
            DbPool::Sqlite(_) => Dialect::Sqlite,
            DbPool::Postgres(_) => Dialect::Postgres,
        }
    }

    /// Construct the matching `DialectAdapter` for this pool variant.
    pub fn adapter(&self) -> Box<dyn DialectAdapter> {
        match self {
            DbPool::Sqlite(_) => Box::new(SqliteDialect),
            DbPool::Postgres(_) => Box::new(PostgresDialect),
        }
    }

    /// Close the pool gracefully.
    pub async fn close(&self) {
        match self {
            DbPool::Sqlite(p) => p.close().await,
            DbPool::Postgres(p) => p.close().await,
        }
    }

    /// Borrow the SQLite pool. Returns Err if backend is Postgres.
    /// Stores not yet migrated to dual-backend dispatch use this accessor
    /// to keep their `&SqlitePool` parameter type during PR 11.1.
    pub fn as_sqlite(&self) -> Result<&SqlitePool> {
        match self {
            DbPool::Sqlite(p) => Ok(p),
            DbPool::Postgres(_) => Err(DbError::Query(
                "store requires SQLite backend; Postgres dispatch lands in PR 11.2/11.3".into(),
            )
            .into()),
        }
    }
}

/// Database connections bundle for per-agent databases.
pub struct Db {
    /// Backend-typed SQL pool. Wrapped in `Arc` so stores can clone.
    pub pool: Arc<DbPool>,

    /// LanceDB connection for vector storage.
    pub lance: lancedb::Connection,

    /// Redb database for key-value config.
    pub redb: Arc<redb::Database>,
}

impl Db {
    /// Connect to all databases and run migrations.
    ///
    /// `db_url` selects the backend at runtime:
    /// - `None` falls back to per-agent SQLite under `data_dir/agent.db`.
    /// - `Some(DatabaseUrl::Sqlite(...))` connects to the named SQLite file.
    /// - `Some(DatabaseUrl::Postgres(...))` errors in PR 11.1 because
    ///   `migrations/postgres/` is not yet shipped. PR 11.3 lands the
    ///   Postgres migration tree and unblocks this path.
    pub async fn connect(data_dir: &Path, db_url: Option<&DatabaseUrl>) -> Result<Self> {
        let pool = connect_per_agent_pool(data_dir, db_url).await?;

        let lance_path = data_dir.join("lancedb");
        std::fs::create_dir_all(&lance_path).with_context(|| {
            format!(
                "failed to create LanceDB directory: {}",
                lance_path.display()
            )
        })?;
        let lance = lancedb::connect(lance_path.to_str().unwrap_or("./lancedb"))
            .execute()
            .await
            .map_err(|e| DbError::LanceConnect(e.to_string()))?;

        let redb_path = data_dir.join("config.redb");
        let redb = open_config_redb_with_retry(&redb_path).await?;

        Ok(Self {
            pool: Arc::new(pool),
            lance,
            redb: Arc::new(redb),
        })
    }

    /// Borrow the underlying SQLite pool. Returns Err if backend is Postgres.
    /// PR 11.1 stores remain on `&SqlitePool` parameter type and use this
    /// accessor; PR 11.2/11.3 migrate stores to take `Arc<DbPool>` directly.
    pub fn sqlite_pool(&self) -> Result<&SqlitePool> {
        self.pool.as_sqlite()
    }

    /// Close all database connections gracefully.
    pub async fn close(self) {
        self.pool.close().await;
        // LanceDB and redb close automatically when dropped
    }
}

/// Open `config.redb`, retrying briefly on `DatabaseAlreadyOpen`.
///
/// Containerized restarts can leave a redb lockfile in a transient state
/// that resolves once the prior process's flock is fully released. Rather
/// than CrashLoopBackOff through the kernel-driven cleanup, this helper
/// retries up to 5 times with exponential backoff (200ms, 400ms, 800ms,
/// 1600ms, 3200ms — total worst case 6.2s, well under K8s readiness
/// windows). Other `DatabaseError` variants surface immediately because
/// they don't represent a transient lock collision.
async fn open_config_redb_with_retry(path: &Path) -> Result<redb::Database> {
    use std::time::Duration;

    const MAX_RETRIES: u32 = 5;

    let mut attempt: u32 = 0;
    loop {
        tracing::info!(
            path = %path.display(),
            attempt,
            "opening config.redb"
        );
        match redb::Database::create(path) {
            Ok(db) => return Ok(db),
            Err(redb::DatabaseError::DatabaseAlreadyOpen) if attempt < MAX_RETRIES => {
                let delay_ms: u64 = 200u64 << attempt;
                tracing::warn!(
                    path = %path.display(),
                    attempt,
                    delay_ms,
                    "config.redb DatabaseAlreadyOpen, retrying"
                );
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                attempt += 1;
            }
            Err(redb::DatabaseError::DatabaseAlreadyOpen) => {
                let attempts = attempt + 1;
                tracing::error!(
                    path = %path.display(),
                    attempts,
                    "config.redb still locked after retry budget exhausted; another process likely holds the flock"
                );
                return Err(anyhow::anyhow!(
                    "config.redb at {} still locked after {} attempts (DatabaseAlreadyOpen); another process likely holds the flock",
                    path.display(),
                    attempts
                )
                .into());
            }
            Err(error) => {
                tracing::error!(
                    path = %path.display(),
                    %error,
                    "non-retryable error opening config.redb"
                );
                return Err(anyhow::Error::from(error)
                    .context(format!(
                        "failed to create redb at: {} (non-retryable error)",
                        path.display()
                    ))
                    .into());
            }
        }
    }
}

/// Connect to the instance-level spacebot database and run its migrations.
///
/// Returns `Arc<DbPool>` rather than the raw sqlx pool so callers can hold
/// the variant tag alongside the pool. The instance database lives at
/// `{instance_dir}/data/spacebot.db` for SQLite mode and is the cluster
/// Postgres database for Postgres mode.
///
/// `db_url` semantics match `Db::connect`. `None` falls back to instance
/// SQLite under `data_dir/spacebot.db`. Postgres URLs error in PR 11.1
/// pending the Postgres migration tree.
///
/// If an old `tasks.db` exists from before the rename, it is moved to
/// `spacebot.db` first.
pub async fn connect_instance_db(
    data_dir: &Path,
    db_url: Option<&DatabaseUrl>,
) -> Result<Arc<DbPool>> {
    std::fs::create_dir_all(data_dir)
        .with_context(|| format!("failed to create data directory: {}", data_dir.display()))?;

    let pool = connect_instance_pool(data_dir, db_url).await?;
    Ok(Arc::new(pool))
}

/// Which migration tree to run on connect. PR 11.1 only knows the SQLite
/// trees; PR 11.2 + PR 11.3 add Postgres-side variants.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum MigrationsTree {
    /// Per-agent SQLite migrations at `migrations/`.
    PerAgent,
    /// Instance-wide SQLite migrations at `migrations/global/`.
    Instance,
}

impl MigrationsTree {
    fn sqlite_path(self) -> &'static str {
        match self {
            MigrationsTree::PerAgent => "migrations",
            MigrationsTree::Instance => "migrations/global",
        }
    }

    /// Postgres migrations directory companion to `sqlite_path()`. Returns
    /// `None` for trees that don't yet have a Postgres tree on disk; the
    /// pool-open path treats `None` as "Postgres unsupported for this tier
    /// in this PR" and returns a structured error pointing the operator at
    /// the next PR that lands the missing tree. PR 11.2 ships
    /// `migrations/postgres/global/`; PR 11.3 ships
    /// `migrations/postgres/`.
    fn postgres_path(self) -> Option<&'static str> {
        match self {
            MigrationsTree::PerAgent => None,
            MigrationsTree::Instance => Some("migrations/postgres/global"),
        }
    }
}

async fn connect_per_agent_pool(data_dir: &Path, db_url: Option<&DatabaseUrl>) -> Result<DbPool> {
    let resolved = resolve_per_agent_url(data_dir, db_url)?;
    open_pool_and_migrate(&resolved, MigrationsTree::PerAgent).await
}

async fn connect_instance_pool(data_dir: &Path, db_url: Option<&DatabaseUrl>) -> Result<DbPool> {
    let resolved = resolve_instance_url(data_dir, db_url)?;
    open_pool_and_migrate(&resolved, MigrationsTree::Instance).await
}

/// Open a pool for the given URL variant and run migrations from the
/// matching directory tree. Uses `sqlx::migrate::Migrator::new(Path)` for
/// runtime directory selection so migrations are loaded from disk at
/// startup rather than embedded in the binary. This trades binary
/// self-containment for runtime backend switching, which is the right
/// call for daemon deployments that ship with their `migrations/`
/// directory adjacent.
///
/// PR 11.2 unblocks the Postgres arm for `MigrationsTree::Instance` only.
/// `MigrationsTree::PerAgent` keeps hard-erroring until PR 11.3 ships
/// `migrations/postgres/`.
async fn open_pool_and_migrate(url: &DatabaseUrl, tree: MigrationsTree) -> Result<DbPool> {
    match url {
        DatabaseUrl::Sqlite(s) => {
            let pool = SqlitePool::connect(s.as_str()).await.with_context(|| {
                format!("failed to connect to SQLite: {}", redact_url(s.as_str()))
            })?;
            let migrator = sqlx::migrate::Migrator::new(std::path::Path::new(tree.sqlite_path()))
                .await
                .with_context(|| {
                    format!("failed to load migrations from {}", tree.sqlite_path())
                })?;
            migrator
                .run(&pool)
                .await
                .with_context(|| format!("failed to run {} migrations", tree.sqlite_path()))?;
            Ok(DbPool::Sqlite(pool))
        }
        DatabaseUrl::Postgres(p) => {
            let path = tree.postgres_path().ok_or_else(|| {
                DbError::Other(anyhow::anyhow!(
                    "Postgres backend selected but migrations/postgres/ does not exist for the \
                     per-agent tier. PR 11.3 will ship `migrations/postgres/` for per-agent stores."
                ))
            })?;
            let pool = PgPool::connect(p.as_str()).await.with_context(|| {
                format!("failed to connect to Postgres: {}", redact_url(p.as_str()))
            })?;
            let migrator = sqlx::migrate::Migrator::new(std::path::Path::new(path))
                .await
                .with_context(|| format!("failed to load migrations from {path}"))?;
            migrator
                .run(&pool)
                .await
                .with_context(|| format!("failed to run {path} migrations"))?;
            Ok(DbPool::Postgres(pool))
        }
    }
}

/// Resolve a per-agent database URL into a `DatabaseUrl`.
///
/// `db_url = None` falls back to today's behavior: per-agent SQLite at
/// `data_dir/agent.db`, including the legacy `spacebot.db` rename. A
/// user-supplied URL bypasses the legacy rename.
fn resolve_per_agent_url(data_dir: &Path, db_url: Option<&DatabaseUrl>) -> Result<DatabaseUrl> {
    if let Some(url) = db_url {
        return Ok(url.clone());
    }

    let agent_db = data_dir.join("agent.db");
    let legacy_db = data_dir.join("spacebot.db");
    if legacy_db.exists() && !agent_db.exists() {
        std::fs::rename(&legacy_db, &agent_db).with_context(|| {
            format!(
                "failed to rename legacy per-agent DB {} -> {}",
                legacy_db.display(),
                agent_db.display()
            )
        })?;
    }
    let url = format!("sqlite:{}?mode=rwc", agent_db.display());
    DatabaseUrl::from_str(&url).map_err(Into::into)
}

/// Resolve an instance-pool database URL into a `DatabaseUrl`.
fn resolve_instance_url(data_dir: &Path, db_url: Option<&DatabaseUrl>) -> Result<DatabaseUrl> {
    if let Some(url) = db_url {
        return Ok(url.clone());
    }

    let db_path = data_dir.join("spacebot.db");
    let legacy_tasks_db = data_dir.join("tasks.db");
    if legacy_tasks_db.exists() && !db_path.exists() {
        std::fs::rename(&legacy_tasks_db, &db_path).with_context(|| {
            format!(
                "failed to rename legacy tasks.db -> spacebot.db at {}",
                data_dir.display()
            )
        })?;
    }
    let url = format!("sqlite:{}?mode=rwc", db_path.display());
    DatabaseUrl::from_str(&url).map_err(Into::into)
}

/// Strip user:pass credentials from a connection URL before embedding it in
/// log lines, error messages, or panic strings. Replaces `user:pass@` with
/// `***:***@`. Targets the realistic threat: a typo'd `postgres://` URL
/// surfacing through `DbError::UnsupportedScheme` or a connect failure
/// echoing the URL into operator logs.
///
/// Limitations: does not redact query-string credentials (`?password=...`)
/// or URL-encoded user:pass forms. Sufficient for PR 11.1's two callers.
fn redact_url(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else {
        return url.to_string();
    };
    let after_scheme = scheme_end + 3;
    let Some(at_offset) = url[after_scheme..].find('@') else {
        return url.to_string();
    };
    let at = after_scheme + at_offset;
    format!("{}***:***{}", &url[..after_scheme], &url[at..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_url_parses_sqlite_scheme() {
        let url: DatabaseUrl = "sqlite:/tmp/x.db".parse().unwrap();
        assert_eq!(url.dialect(), Dialect::Sqlite);
        assert_eq!(url.as_str(), "sqlite:/tmp/x.db");
    }

    #[test]
    fn database_url_parses_postgres_scheme() {
        let url: DatabaseUrl = "postgres://user:pass@host:5432/db".parse().unwrap();
        assert_eq!(url.dialect(), Dialect::Postgres);
    }

    #[test]
    fn database_url_parses_postgresql_scheme() {
        let url: DatabaseUrl = "postgresql://user:pass@host:5432/db".parse().unwrap();
        assert_eq!(url.dialect(), Dialect::Postgres);
    }

    #[test]
    fn database_url_rejects_unknown_scheme() {
        let err: DbError = "mysql://x".parse::<DatabaseUrl>().unwrap_err();
        assert!(err.to_string().contains("mysql://x"));
    }

    #[test]
    fn database_url_redacts_credentials_in_unsupported_scheme_error() {
        let err: DbError = "potgres://user:secret@host:5432/db"
            .parse::<DatabaseUrl>()
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            !msg.contains("secret"),
            "credentials leaked into UnsupportedScheme error: {msg}"
        );
        assert!(
            !msg.contains("user"),
            "user component leaked into UnsupportedScheme error: {msg}"
        );
        assert!(msg.contains("***:***@host:5432/db"));
        assert!(msg.contains("potgres://"));
    }

    #[test]
    fn database_url_deserializes_via_serde() {
        let url: DatabaseUrl = serde_json::from_str(r#""sqlite:/tmp/x.db""#).unwrap();
        assert_eq!(url.dialect(), Dialect::Sqlite);

        let err = serde_json::from_str::<DatabaseUrl>(r#""mysql://x""#).unwrap_err();
        assert!(err.to_string().contains("mysql://x"));
    }

    #[test]
    fn redact_url_passes_credentialless_urls_through() {
        assert_eq!(redact_url("sqlite:/tmp/x.db"), "sqlite:/tmp/x.db");
        assert_eq!(redact_url("sqlite::memory:"), "sqlite::memory:");
        assert_eq!(
            redact_url("postgres://host:5432/db"),
            "postgres://host:5432/db"
        );
    }

    #[test]
    fn redact_url_redacts_user_and_password() {
        assert_eq!(
            redact_url("postgres://alice:hunter2@host:5432/db"),
            "postgres://***:***@host:5432/db"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn dbpool_sqlite_variant_dialect_and_adapter() {
        let inner = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        let pool = DbPool::Sqlite(inner);
        assert_eq!(pool.dialect(), Dialect::Sqlite);
        assert_eq!(pool.adapter().now_expr(), "datetime('now')");
        assert!(pool.as_sqlite().is_ok());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn dbpool_postgres_variant_as_sqlite_returns_err() {
        let inner = sqlx::PgPool::connect_lazy("postgres://nope:5432/x")
            .expect("connect_lazy parses URL without connecting");
        let pool = DbPool::Postgres(inner);
        assert_eq!(pool.dialect(), Dialect::Postgres);
        assert_eq!(pool.adapter().now_expr(), "now()");
        let err = pool.as_sqlite().unwrap_err();
        assert!(
            err.to_string().contains("requires SQLite backend"),
            "expected SQLite-backend error, got: {err}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn db_connect_with_sqlite_url_runs_migrations() {
        let tmp = tempfile::tempdir().unwrap();
        let url: DatabaseUrl = format!("sqlite:{}/agent.db?mode=rwc", tmp.path().display())
            .parse()
            .unwrap();
        let db = Db::connect(tmp.path(), Some(&url)).await.unwrap();
        let pool = db.sqlite_pool().unwrap();
        let row: (i64,) = sqlx::query_as("SELECT count(*) FROM _sqlx_migrations")
            .fetch_one(pool)
            .await
            .unwrap();
        assert!(row.0 > 0, "expected at least one migration applied");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn db_connect_with_postgres_url_fails_fast_with_pr_pointer() {
        let tmp = tempfile::tempdir().unwrap();
        let url: DatabaseUrl = "postgres://nope:5432/x".parse().unwrap();
        let err = match Db::connect(tmp.path(), Some(&url)).await {
            Ok(_) => panic!("expected postgres URL to fail at connect"),
            Err(e) => e,
        };
        let msg = err.to_string();
        assert!(
            msg.contains("PR 11.3"),
            "expected per-agent fail-fast message to point at PR 11.3, got: {msg}"
        );
    }

    /// PR 11.2 unblocked the instance-tier Postgres dispatch. The fail-fast
    /// message that PR 11.1 emitted for ALL Postgres URLs no longer fires
    /// when `connect_instance_db` is the entry point. We can't actually
    /// connect to a real Postgres in unit-test scope (testcontainers is
    /// reserved for `tests/instance_postgres.rs`), so the contract verified
    /// here is: the error must NOT be the PR 11.3 fail-fast message — the
    /// dispatch must reach the connect / migration-loading step downstream.
    #[tokio::test(flavor = "multi_thread")]
    async fn connect_instance_db_with_postgres_url_attempts_connect() {
        let tmp = tempfile::tempdir().unwrap();
        let url: DatabaseUrl = "postgres://nobody@127.0.0.1:1/neverexists".parse().unwrap();
        let err = match connect_instance_db(tmp.path(), Some(&url)).await {
            Ok(_) => panic!("expected connect to fail without a real Postgres"),
            Err(e) => e,
        };
        let msg = err.to_string();
        assert!(
            !msg.contains("PR 11.3"),
            "instance-tier Postgres should no longer fail-fast on PR 11.3 message; got: {msg}"
        );
        assert!(
            msg.contains("Postgres") || msg.contains("connection") || msg.contains("migrations"),
            "expected connect/migration-related error, got: {msg}"
        );
    }

    // R5: Legacy DB rename coverage. resolve_per_agent_url and
    // resolve_instance_url run std::fs::rename against user data on every
    // upgrade. If the precondition logic ever inverts, users lose data on
    // upgrade. These tests pin the four branches of each rename helper.

    #[test]
    fn resolve_per_agent_url_renames_legacy_when_target_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy = tmp.path().join("spacebot.db");
        std::fs::write(&legacy, b"legacy-marker").unwrap();
        let resolved = resolve_per_agent_url(tmp.path(), None).unwrap();
        assert_eq!(resolved.dialect(), Dialect::Sqlite);
        assert!(resolved.as_str().contains("agent.db"));
        assert!(!legacy.exists(), "legacy file must be moved");
        let target = tmp.path().join("agent.db");
        assert!(target.exists(), "renamed target must exist");
        assert_eq!(std::fs::read(&target).unwrap(), b"legacy-marker");
    }

    #[test]
    fn resolve_per_agent_url_skips_rename_when_both_present() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy = tmp.path().join("spacebot.db");
        let target = tmp.path().join("agent.db");
        std::fs::write(&legacy, b"legacy").unwrap();
        std::fs::write(&target, b"current").unwrap();
        let _ = resolve_per_agent_url(tmp.path(), None).unwrap();
        // Both must remain untouched: rename runs only when target is absent.
        assert_eq!(std::fs::read(&legacy).unwrap(), b"legacy");
        assert_eq!(std::fs::read(&target).unwrap(), b"current");
    }

    #[test]
    fn resolve_per_agent_url_no_op_when_legacy_absent() {
        let tmp = tempfile::tempdir().unwrap();
        // No legacy, no target. Fresh install case.
        let resolved = resolve_per_agent_url(tmp.path(), None).unwrap();
        assert!(resolved.as_str().contains("agent.db"));
        // No file should have been created by the resolve step itself
        // (Db::connect creates it during SqlitePool::connect, not here).
        assert!(!tmp.path().join("spacebot.db").exists());
    }

    #[test]
    fn resolve_per_agent_url_skips_rename_when_url_supplied() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy = tmp.path().join("spacebot.db");
        std::fs::write(&legacy, b"legacy-must-not-move").unwrap();
        let supplied: DatabaseUrl = "sqlite:/somewhere/else.db".parse().unwrap();
        let resolved = resolve_per_agent_url(tmp.path(), Some(&supplied)).unwrap();
        assert_eq!(resolved.as_str(), "sqlite:/somewhere/else.db");
        // Operator-supplied URL bypasses the rename logic entirely.
        assert!(
            legacy.exists(),
            "legacy must not be touched when url supplied"
        );
    }

    #[test]
    fn resolve_instance_url_renames_legacy_tasks_db_when_target_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy = tmp.path().join("tasks.db");
        std::fs::write(&legacy, b"legacy-tasks").unwrap();
        let resolved = resolve_instance_url(tmp.path(), None).unwrap();
        assert!(resolved.as_str().contains("spacebot.db"));
        assert!(!legacy.exists(), "legacy tasks.db must be moved");
        let target = tmp.path().join("spacebot.db");
        assert!(target.exists());
        assert_eq!(std::fs::read(&target).unwrap(), b"legacy-tasks");
    }

    #[test]
    fn resolve_instance_url_skips_rename_when_both_present() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy = tmp.path().join("tasks.db");
        let target = tmp.path().join("spacebot.db");
        std::fs::write(&legacy, b"legacy").unwrap();
        std::fs::write(&target, b"current").unwrap();
        let _ = resolve_instance_url(tmp.path(), None).unwrap();
        assert_eq!(std::fs::read(&legacy).unwrap(), b"legacy");
        assert_eq!(std::fs::read(&target).unwrap(), b"current");
    }

    #[test]
    fn resolve_instance_url_no_op_when_legacy_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let resolved = resolve_instance_url(tmp.path(), None).unwrap();
        assert!(resolved.as_str().contains("spacebot.db"));
        assert!(!tmp.path().join("tasks.db").exists());
    }

    #[test]
    fn resolve_instance_url_skips_rename_when_url_supplied() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy = tmp.path().join("tasks.db");
        std::fs::write(&legacy, b"legacy-must-not-move").unwrap();
        let supplied: DatabaseUrl = "sqlite:/elsewhere.db".parse().unwrap();
        let resolved = resolve_instance_url(tmp.path(), Some(&supplied)).unwrap();
        assert_eq!(resolved.as_str(), "sqlite:/elsewhere.db");
        assert!(
            legacy.exists(),
            "legacy must not be touched when url supplied"
        );
    }

    // R5: connect_instance_db has distinct behavior from Db::connect
    // (uses MigrationsTree::Instance → migrations/global/, returns bare
    // Arc<DbPool>). Cover the SQLite happy path.

    #[tokio::test(flavor = "multi_thread")]
    async fn connect_instance_db_with_sqlite_url_runs_global_migrations() {
        let tmp = tempfile::tempdir().unwrap();
        let url: DatabaseUrl = format!("sqlite:{}/spacebot.db?mode=rwc", tmp.path().display())
            .parse()
            .unwrap();
        let pool = connect_instance_db(tmp.path(), Some(&url)).await.unwrap();
        let sqlite = pool.as_sqlite().unwrap();
        let row: (i64,) = sqlx::query_as("SELECT count(*) FROM _sqlx_migrations")
            .fetch_one(sqlite)
            .await
            .unwrap();
        assert!(
            row.0 > 0,
            "expected at least one migrations/global migration applied"
        );
    }

    // R5: Close paths. Db::close consumes self and awaits pool.close.
    // Verify the pool reports closed afterward.

    #[tokio::test(flavor = "multi_thread")]
    async fn dbpool_sqlite_close_marks_pool_closed() {
        let inner = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        let pool_handle = inner.clone();
        let pool = DbPool::Sqlite(inner);
        pool.close().await;
        assert!(
            pool_handle.is_closed(),
            "pool must report closed after close()"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn db_close_marks_underlying_pool_closed() {
        let tmp = tempfile::tempdir().unwrap();
        let url: DatabaseUrl = format!("sqlite:{}/agent.db?mode=rwc", tmp.path().display())
            .parse()
            .unwrap();
        let db = Db::connect(tmp.path(), Some(&url)).await.unwrap();
        let pool_handle = db.sqlite_pool().unwrap().clone();
        db.close().await;
        assert!(pool_handle.is_closed(), "underlying pool must close");
    }
}
