//! Integration test for PR 11.2 — instance-tier Postgres support.
//!
//! Spins up `postgres:16-alpine` via testcontainers, applies the
//! `migrations/postgres/global/` tree, exercises each instance-tier store
//! (TaskStore, WikiStore, NotificationStore, ProjectStore) and validates
//! the audit hash chain end-to-end on the Postgres backend.
//!
//! These tests require Docker. When Docker is unavailable (no daemon
//! running) the testcontainers crate returns a clear error at startup and
//! the suite fails fast rather than silently passing. CI runners ship
//! with Docker; the `test-postgres-instance` job (added in Task 18) wires
//! this file in.

use std::str::FromStr;
use std::sync::Arc;

use spacebot::audit::AuditAppender;
use spacebot::audit::types::{AuditAction, AuditEvent};
use spacebot::db::{DatabaseUrl, DbPool};
use spacebot::notifications::{
    NewNotification, NotificationKind, NotificationSeverity, NotificationStore,
};
use spacebot::projects::store::{CreateProjectInput, ProjectStore};
use spacebot::tasks::store::{CreateTaskInput, TaskPriority, TaskStatus, TaskStore};
use spacebot::wiki::{CreateWikiPageInput, WikiPageType, WikiStore};
use sqlx::PgPool;
use testcontainers::ContainerAsync;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;

/// Spin up a fresh `postgres:16-alpine` container, apply the
/// `migrations/postgres/global/` tree, and return an `Arc<DbPool>` plus
/// the container handle (kept alive for the lifetime of the test).
async fn setup_postgres() -> (Arc<DbPool>, ContainerAsync<Postgres>) {
    let container = Postgres::default()
        .start()
        .await
        .expect("failed to start postgres testcontainer (docker daemon required)");
    let host = container
        .get_host()
        .await
        .expect("read container host");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("read container 5432 port mapping");
    let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");
    // Validate the URL through the project's typed parser so the test
    // exercises the same DatabaseUrl::from_str path the daemon uses.
    let _parsed = DatabaseUrl::from_str(&url).expect("DatabaseUrl::from_str");
    let pg_pool = PgPool::connect(&url)
        .await
        .expect("connect to testcontainer postgres");
    sqlx::migrate::Migrator::new(std::path::Path::new("migrations/postgres/global"))
        .await
        .expect("load migrations/postgres/global tree")
        .run(&pg_pool)
        .await
        .expect("apply postgres migrations");
    (Arc::new(DbPool::Postgres(pg_pool)), container)
}

#[tokio::test(flavor = "multi_thread")]
async fn migrations_apply_cleanly() {
    // Reaching this assertion means all 14 migrations under
    // migrations/postgres/global/ applied without error against
    // postgres:16-alpine.
    let (_pool, _container) = setup_postgres().await;
}

/// Audit hash chain end-to-end on Postgres. Mirrors the SQLite-side
/// assertion from `tests/audit_chain.rs`: 10 events appended in a row,
/// `verify_chain()` returns valid + total_rows = 10.
///
/// This is the marquee acceptance test for PR 11.2: it proves the per-
/// backend transaction in `AuditAppender::append` works on the Postgres
/// arm and that the canonical-bytes computation produces the same hash
/// the SQLite path would (the input is identical, so the SHA-256 must
/// match byte-for-byte).
#[tokio::test(flavor = "multi_thread")]
async fn audit_hash_chain_validates_against_postgres() {
    let (pool, _container) = setup_postgres().await;
    let appender = AuditAppender::new_for_tests_pg(pool.clone());

    for i in 0..10 {
        let event = AuditEvent {
            principal_key: format!("00000000-0000-0000-0000-000000000000:user-{i}"),
            principal_type: "user".to_string(),
            action: AuditAction::ResourceRead,
            resource_type: Some("memory".to_string()),
            resource_id: Some(format!("mem-{i}")),
            result: "allow".to_string(),
            source_ip: None,
            request_id: None,
            metadata: serde_json::json!({"i": i}),
        };
        appender.append(event).await.expect("append failed");
    }

    let result = appender.verify_chain().await.expect("verify failed");
    assert!(
        result.valid,
        "chain should be valid; mismatch at seq {:?}",
        result.first_mismatch_seq
    );
    assert_eq!(result.total_rows, 10);
}

#[tokio::test(flavor = "multi_thread")]
async fn task_store_crud_against_postgres() {
    let (pool, _container) = setup_postgres().await;
    let store = TaskStore::new(pool);

    let input = CreateTaskInput {
        owner_agent_id: "alice".into(),
        assigned_agent_id: "alice".into(),
        title: "test".into(),
        description: None,
        status: TaskStatus::Backlog,
        priority: TaskPriority::Medium,
        subtasks: vec![],
        metadata: serde_json::json!({}),
        source_memory_id: None,
        created_by: "human".into(),
    };

    let task = store.create(input).await.expect("create task");
    let fetched = store
        .get_by_number(task.task_number)
        .await
        .expect("get task by number")
        .expect("task row present");
    assert_eq!(fetched.id, task.id);
    assert_eq!(fetched.task_number, task.task_number);
}

/// WikiStore::search on Postgres uses the tsvector STORED column +
/// websearch_to_tsquery('english', $1). This test seeds two pages with
/// distinguishable English-language content, queries for a token that
/// only appears in one, and asserts exactly one match.
///
/// The most distinctive PR 11.2 test: it exercises the per-backend search
/// dispatch where SQLite uses FTS5 MATCH and Postgres uses tsvector @@.
/// A regression that left the SQLite-shaped query on the Postgres arm
/// would fail at runtime (no `wiki_pages_fts` virtual table on Postgres).
#[tokio::test(flavor = "multi_thread")]
async fn wiki_store_search_against_postgres_tsvector() {
    let (pool, _container) = setup_postgres().await;
    let store = WikiStore::new(pool);

    store
        .create(CreateWikiPageInput {
            title: "fast database queries".into(),
            page_type: WikiPageType::Reference,
            content: "indexing strategies for fast lookups".into(),
            related: vec![],
            author_type: "user".into(),
            author_id: "alice".into(),
            edit_summary: None,
        })
        .await
        .expect("create page 1");
    store
        .create(CreateWikiPageInput {
            title: "slow filesystem scans".into(),
            page_type: WikiPageType::Reference,
            content: "rotational disk seeks".into(),
            related: vec![],
            author_type: "user".into(),
            author_id: "alice".into(),
            edit_summary: None,
        })
        .await
        .expect("create page 2");

    let hits = store.search("fast", None).await.expect("search");
    assert_eq!(
        hits.len(),
        1,
        "tsvector match should find only the page mentioning 'fast'"
    );
    assert!(
        hits[0].slug.contains("fast"),
        "matched slug should be the 'fast' page, got {:?}",
        hits[0].slug
    );
}

/// NotificationStore::insert on Postgres uses
/// `INSERT ... ON CONFLICT DO NOTHING`, mirroring the SQLite arm's
/// `INSERT OR IGNORE`. The partial unique index
/// `idx_notifications_entity_active` keys on
/// `(related_entity_type, related_entity_id)` for non-dismissed rows, so
/// inserting the same entity twice should yield Some + None.
#[tokio::test(flavor = "multi_thread")]
async fn notification_store_insert_via_on_conflict_do_nothing() {
    let (pool, _container) = setup_postgres().await;
    let store = NotificationStore::new(pool);

    let n = NewNotification {
        kind: NotificationKind::TaskApproval,
        severity: NotificationSeverity::Info,
        title: "approve me".into(),
        body: None,
        agent_id: None,
        related_entity_type: Some("task".into()),
        related_entity_id: Some("42".into()),
        action_url: None,
        metadata: None,
    };

    let first = store.insert(n.clone()).await.expect("first insert");
    let second = store.insert(n).await.expect("second insert");
    assert!(first.is_some(), "first insert should produce a row");
    assert!(
        second.is_none(),
        "duplicate insert on the partial unique index must return None \
         (ON CONFLICT DO NOTHING)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn project_store_crud_against_postgres() {
    let (pool, _container) = setup_postgres().await;
    let store = ProjectStore::new(pool);

    let project = store
        .create_project(CreateProjectInput {
            name: "test-project".into(),
            description: "smoke test".into(),
            icon: "🧪".into(),
            tags: vec!["test".into()],
            root_path: "/tmp/test-project".into(),
            settings: serde_json::json!({}),
        })
        .await
        .expect("create project");
    let fetched = store
        .get_project(&project.id)
        .await
        .expect("get project")
        .expect("project row present");
    assert_eq!(fetched.id, project.id);
    assert_eq!(fetched.name, "test-project");
}
