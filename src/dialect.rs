//! SQL dialect adaptation for SQLite and Postgres backends.
//!
//! The dialect adapter holds the small set of SQL string differences between
//! SQLite and Postgres that don't warrant a full match arm in store-level
//! dispatch. These are concentrated at table-DDL sites: `AUTOINCREMENT` vs
//! `BIGSERIAL`, `datetime('now')` vs `now()`, `TEXT` vs `JSONB`.
//!
//! Backend selection happens in `db::Db::connect` based on the connection-
//! string scheme. The `DialectAdapter` accompanies each `DbPool` variant so
//! migration code and DDL helpers can branch without re-resolving the URL.
//!
//! Per-query SQL differences (placeholder syntax `?` vs `$1`, JSON operators,
//! `RETURNING` clauses) live inside per-variant match arms in the stores
//! themselves, NOT in this trait. The trait is a small companion abstraction,
//! not the load-bearing dispatch layer.

/// SQL dialect adapter for cross-backend DDL string differences.
pub trait DialectAdapter: Send + Sync + std::fmt::Debug {
    /// Expression for "current timestamp" in DEFAULT clauses and SQL bodies.
    fn now_expr(&self) -> &'static str;

    /// Column declaration for an auto-incrementing integer primary key.
    fn autoincrement_pk(&self) -> &'static str;

    /// Column type for JSON storage (TEXT on SQLite, JSONB on Postgres).
    fn json_type(&self) -> &'static str;
}

/// SQLite dialect adapter.
#[derive(Debug)]
pub struct SqliteDialect;

impl DialectAdapter for SqliteDialect {
    fn now_expr(&self) -> &'static str {
        "datetime('now')"
    }

    fn autoincrement_pk(&self) -> &'static str {
        "INTEGER PRIMARY KEY AUTOINCREMENT"
    }

    fn json_type(&self) -> &'static str {
        "TEXT"
    }
}

/// Postgres dialect adapter.
#[derive(Debug)]
pub struct PostgresDialect;

impl DialectAdapter for PostgresDialect {
    fn now_expr(&self) -> &'static str {
        "now()"
    }

    fn autoincrement_pk(&self) -> &'static str {
        "BIGSERIAL PRIMARY KEY"
    }

    fn json_type(&self) -> &'static str {
        "JSONB"
    }
}
