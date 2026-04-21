//! Integration test: Phase 2 migrations + repository helpers against a
//! real SQLite instance. Uses the crate's global-migration path.

use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::principals::Visibility;
use spacebot::auth::repository::{
    RepositoryError, get_ownership, set_ownership, upsert_team, upsert_user_from_auth,
};
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;

async fn setup_pool() -> sqlx::SqlitePool {
    // :memory: in-process for each test. Migrations run against it.
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("connect memory sqlite");

    // Migrations directory (instance-level):
    sqlx::migrate!("./migrations/global")
        .run(&pool)
        .await
        .expect("run global migrations");
    pool
}

fn make_ctx(tid: &str, oid: &str) -> AuthContext {
    AuthContext {
        principal_type: PrincipalType::User,
        tid: Arc::from(tid),
        oid: Arc::from(oid),
        roles: vec![],
        groups: vec![],
        groups_overage: false,
        display_email: Some(Arc::from(format!("{oid}@example.com").as_str())),
        display_name: Some(Arc::from(format!("User {oid}").as_str())),
    }
}

#[tokio::test]
async fn migrations_apply_cleanly() {
    let _pool = setup_pool().await;
    // If we got here, all global migrations applied without error.
}

#[tokio::test]
async fn upsert_user_is_idempotent() {
    let pool = setup_pool().await;
    let ctx = make_ctx("tid-1", "oid-a");
    let first = upsert_user_from_auth(&pool, &ctx)
        .await
        .expect("first upsert");
    let second = upsert_user_from_auth(&pool, &ctx)
        .await
        .expect("second upsert");
    assert_eq!(first.principal_key, second.principal_key);
    assert_eq!(
        first.created_at, second.created_at,
        "created_at must be stable"
    );
    assert!(
        second.updated_at >= first.updated_at,
        "updated_at should not regress"
    );
}

#[tokio::test]
async fn team_upsert_keys_on_external_id() {
    let pool = setup_pool().await;
    let t1 = upsert_team(&pool, "grp-111", "Platform").await.expect("t1");
    let t2 = upsert_team(&pool, "grp-111", "Platform (renamed)")
        .await
        .expect("t2");
    assert_eq!(t1.id, t2.id);
    assert_eq!(t2.display_name, "Platform (renamed)");
}

#[tokio::test]
async fn ownership_write_then_read_roundtrips() {
    let pool = setup_pool().await;
    let ctx = make_ctx("tid-1", "oid-a");
    upsert_user_from_auth(&pool, &ctx).await.unwrap();
    let key = ctx.principal_key();

    set_ownership(
        &pool,
        "memory",
        "mem-42",
        Some("agent-alpha"),
        &key,
        Visibility::Personal,
        None,
    )
    .await
    .expect("set ownership");

    let got = get_ownership(&pool, "memory", "mem-42")
        .await
        .expect("read ownership")
        .expect("row present");
    assert_eq!(got.owner_principal_key, key);
    assert_eq!(got.visibility, "personal");
    assert_eq!(got.owner_agent_id, Some("agent-alpha".to_string()));
}

#[tokio::test]
async fn team_visibility_requires_team_id() {
    let pool = setup_pool().await;
    let ctx = make_ctx("tid-1", "oid-a");
    upsert_user_from_auth(&pool, &ctx).await.unwrap();
    let key = ctx.principal_key();

    let result = set_ownership(
        &pool,
        "memory",
        "mem-42",
        None,
        &key,
        Visibility::Team,
        None, // missing team_id!
    )
    .await;
    assert!(
        result.is_err(),
        "CHECK constraint must reject team visibility without team_id"
    );
}

#[tokio::test]
async fn upsert_user_rejects_legacy_static_principal() {
    let pool = setup_pool().await;
    let ctx = AuthContext::legacy_static();
    let err = upsert_user_from_auth(&pool, &ctx)
        .await
        .expect_err("legacy_static principals must be rejected");
    assert!(
        matches!(err, RepositoryError::InvalidPrincipalType),
        "expected InvalidPrincipalType, got {err:?}",
    );
}

#[tokio::test]
async fn get_ownership_returns_none_for_missing_row() {
    let pool = setup_pool().await;
    let missing = get_ownership(&pool, "memory", "never-created")
        .await
        .expect("read ownership");
    assert!(
        missing.is_none(),
        "get_ownership must return None, not Err, for an unknown resource",
    );
}

#[tokio::test]
async fn raw_visibility_insert_rejects_unknown_value() {
    // Guards the CHECK (visibility IN ('personal', 'team', 'org')) constraint
    // against SQL paths that bypass the Visibility enum (migrations, admin
    // tooling, backfill scripts). Uses the legacy 'global' value from the
    // research draft, which is the most likely regression vector.
    let pool = setup_pool().await;
    let ctx = make_ctx("tid-1", "oid-a");
    upsert_user_from_auth(&pool, &ctx).await.unwrap();
    let key = ctx.principal_key();

    let result = sqlx::query(
        r#"
        INSERT INTO resource_ownership (
            resource_type, resource_id, owner_principal_key, visibility
        )
        VALUES (?, ?, ?, ?)
        "#,
    )
    .bind("memory")
    .bind("mem-global")
    .bind(&key)
    .bind("global")
    .execute(&pool)
    .await;
    assert!(
        result.is_err(),
        "CHECK constraint must reject visibility = 'global'"
    );
}
