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
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, SqlitePool};

use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

/// Backend dialect selected at connection time. Drives migration directory
/// selection and accompanies the pool for handlers that need to branch on
/// backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Sqlite,
    Postgres,
}

/// Validated database connection URL with backend dialect encoded in the
/// type. Constructed via `FromStr` (or `serde::Deserialize`), which
/// classifies the scheme prefix at parse time. Operator typos surface
/// at config load, not at connect time.
///
/// Use `dialect()` to extract the matching `Dialect` tag, `as_str()` to
/// borrow the inner connection string for sqlx.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(try_from = "String", into = "String")]
pub enum DatabaseUrl {
    Sqlite(String),
    Postgres(String),
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
            DatabaseUrl::Sqlite(s) | DatabaseUrl::Postgres(s) => s.as_str(),
        }
    }
}

impl FromStr for DatabaseUrl {
    type Err = DbError;

    fn from_str(url: &str) -> std::result::Result<Self, Self::Err> {
        if url.starts_with("sqlite:") {
            Ok(DatabaseUrl::Sqlite(url.to_string()))
        } else if url.starts_with("postgres:") || url.starts_with("postgresql:") {
            Ok(DatabaseUrl::Postgres(url.to_string()))
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

impl<'de> Deserialize<'de> for DatabaseUrl {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl From<DatabaseUrl> for String {
    fn from(url: DatabaseUrl) -> String {
        match url {
            DatabaseUrl::Sqlite(s) | DatabaseUrl::Postgres(s) => s,
        }
    }
}

/// Backend-typed connection pool. Each variant holds the native sqlx pool
/// so chrono types, query_as! macros, and FromRow derives work per variant.
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
        let redb = redb::Database::create(&redb_path)
            .with_context(|| format!("failed to create redb at: {}", redb_path.display()))?;

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
/// PR 11.1 hard-errors on Postgres because the `migrations/postgres/`
/// tree is not yet shipped. PR 11.2 / 11.3 land it.
async fn open_pool_and_migrate(url: &DatabaseUrl, tree: MigrationsTree) -> Result<DbPool> {
    match url {
        DatabaseUrl::Sqlite(s) => {
            let pool = SqlitePool::connect(s).await.with_context(|| {
                format!("failed to connect to SQLite: {}", redact_url(s))
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
        DatabaseUrl::Postgres(_) => Err(DbError::Other(anyhow::anyhow!(
            "Postgres backend selected but migrations/postgres/ does not exist. \
             PR 11.2 ships the instance-tier Postgres migrations; \
             PR 11.3 ships the per-agent Postgres migrations."
        ))
        .into()),
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
    Ok(DatabaseUrl::Sqlite(format!(
        "sqlite:{}?mode=rwc",
        agent_db.display()
    )))
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
    Ok(DatabaseUrl::Sqlite(format!(
        "sqlite:{}?mode=rwc",
        db_path.display()
    )))
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
            msg.contains("PR 11.2") || msg.contains("PR 11.3"),
            "expected fail-fast message to point at later PR, got: {msg}"
        );
    }
}
