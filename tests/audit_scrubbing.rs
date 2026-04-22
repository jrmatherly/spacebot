//! Audit insertion MUST scrub secrets from event metadata. Phase 0 Task 0.6
//! installed the regex; here we verify integration.

use spacebot::audit::appender::AuditAppender;
use spacebot::audit::types::{AuditAction, AuditEvent};
use sqlx::sqlite::SqlitePoolOptions;

async fn setup_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new().max_connections(1)
        .connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations/global").run(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn jwt_in_metadata_is_scrubbed() {
    let pool = setup_pool().await;
    let appender = AuditAppender::new_for_tests(pool.clone());
    let jwt = "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";

    let ev = AuditEvent {
        principal_key: "u1".into(),
        principal_type: "user".into(),
        action: AuditAction::AuthFailure,
        resource_type: None,
        resource_id: None,
        result: "denied".into(),
        source_ip: None,
        request_id: None,
        metadata: serde_json::json!({"raw_header": format!("Bearer {jwt}")}),
    };
    let row = appender.append(ev).await.unwrap();
    assert!(!row.metadata_json.contains(jwt),
            "JWT leaked into audit row: {}", row.metadata_json);
}

#[tokio::test]
async fn benign_metadata_is_preserved() {
    let pool = setup_pool().await;
    let appender = AuditAppender::new_for_tests(pool.clone());
    let ev = AuditEvent {
        principal_key: "u1".into(),
        principal_type: "user".into(),
        action: AuditAction::ResourceRead,
        resource_type: Some("memory".into()),
        resource_id: Some("mem-1".into()),
        result: "allowed".into(),
        source_ip: None,
        request_id: None,
        metadata: serde_json::json!({"memory_title": "shopping list"}),
    };
    let row = appender.append(ev).await.unwrap();
    assert!(row.metadata_json.contains("shopping list"),
            "benign content was stripped: {}", row.metadata_json);
}
