use spacebot::audit::{export::{export_audit, ExportConfig, ExportMode}, AuditAppender};
use spacebot::audit::types::{AuditAction, AuditEvent};
use sqlx::sqlite::SqlitePoolOptions;

async fn setup_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new().max_connections(1)
        .connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations/global").run(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn filesystem_export_writes_ndjson_and_manifest() {
    let pool = setup_pool().await;
    let appender = AuditAppender::new_for_tests(pool.clone());
    for i in 0..3 {
        appender.append(AuditEvent {
            principal_key: format!("u{i}"),
            principal_type: "user".into(),
            action: AuditAction::AuthSuccess,
            resource_type: None, resource_id: None,
            result: "allowed".into(),
            source_ip: None, request_id: None,
            metadata: serde_json::json!({"i": i}),
        }).await.unwrap();
    }
    let tmp = tempfile::tempdir().unwrap();
    let cfg = ExportConfig {
        mode: ExportMode::Filesystem { dir: tmp.path().to_path_buf() },
        enabled: true,
    };
    let result = export_audit(&pool, &cfg).await.unwrap();
    assert_eq!(result.rows_exported, 3);

    let entries: Vec<_> = std::fs::read_dir(tmp.path()).unwrap()
        .filter_map(|e| e.ok())
        .collect();
    let ndjson_count = entries.iter()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".ndjson")).count();
    let manifest_count = entries.iter()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".manifest.json")).count();
    assert_eq!(ndjson_count, 1, "one ndjson per export run");
    assert_eq!(manifest_count, 1, "manifest alongside ndjson");
}

#[tokio::test]
async fn export_skipped_when_disabled() {
    let pool = setup_pool().await;
    let tmp = tempfile::tempdir().unwrap();
    let cfg = ExportConfig {
        mode: ExportMode::Filesystem { dir: tmp.path().to_path_buf() },
        enabled: false,
    };
    let result = export_audit(&pool, &cfg).await.unwrap();
    assert_eq!(result.rows_exported, 0);
    let entries: Vec<_> = std::fs::read_dir(tmp.path()).unwrap()
        .filter_map(|e| e.ok()).collect();
    assert!(entries.is_empty());
}
