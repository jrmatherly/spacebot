//! Phase 11: DialectAdapter trait unit tests.

use spacebot::dialect::{DialectAdapter, PostgresDialect, SqliteDialect};

#[test]
fn sqlite_now_expr_returns_datetime_now() {
    let d = SqliteDialect;
    assert_eq!(d.now_expr(), "datetime('now')");
}

#[test]
fn postgres_now_expr_returns_now_paren_paren() {
    let d = PostgresDialect;
    assert_eq!(d.now_expr(), "now()");
}

#[test]
fn sqlite_autoincrement_pk_string() {
    let d = SqliteDialect;
    assert_eq!(d.autoincrement_pk(), "INTEGER PRIMARY KEY AUTOINCREMENT");
}

#[test]
fn postgres_autoincrement_pk_uses_bigserial() {
    let d = PostgresDialect;
    assert_eq!(d.autoincrement_pk(), "BIGSERIAL PRIMARY KEY");
}

#[test]
fn sqlite_json_type_is_text() {
    let d = SqliteDialect;
    assert_eq!(d.json_type(), "TEXT");
}

#[test]
fn postgres_json_type_is_jsonb() {
    let d = PostgresDialect;
    assert_eq!(d.json_type(), "JSONB");
}
