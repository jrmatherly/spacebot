use spacebot::audit::types::{AuditAction, AuditEvent};
use spacebot::audit::{
    AuditAppender,
    export::{ExportConfig, ExportMode, export_audit},
};
use sqlx::sqlite::SqlitePoolOptions;

async fn setup_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations/global")
        .run(&pool)
        .await
        .unwrap();
    pool
}

#[tokio::test]
async fn filesystem_export_writes_ndjson_and_manifest() {
    let pool = setup_pool().await;
    let appender = AuditAppender::new_for_tests(pool.clone());
    for i in 0..3 {
        appender
            .append(AuditEvent {
                principal_key: format!("u{i}"),
                principal_type: "user".into(),
                action: AuditAction::AuthSuccess,
                resource_type: None,
                resource_id: None,
                result: "allowed".into(),
                source_ip: None,
                request_id: None,
                metadata: serde_json::json!({"i": i}),
            })
            .await
            .unwrap();
    }
    let tmp = tempfile::tempdir().unwrap();
    let cfg = ExportConfig {
        mode: ExportMode::Filesystem {
            dir: tmp.path().to_path_buf(),
        },
        enabled: true,
    };
    let result = export_audit(&pool, &cfg).await.unwrap();
    assert_eq!(result.rows_exported, 3);

    let entries: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    let ndjson_count = entries
        .iter()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".ndjson"))
        .count();
    let manifest_count = entries
        .iter()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".manifest.json"))
        .count();
    assert_eq!(ndjson_count, 1, "one ndjson per export run");
    assert_eq!(manifest_count, 1, "manifest alongside ndjson");
}

#[tokio::test]
async fn export_skipped_when_disabled() {
    let pool = setup_pool().await;
    let tmp = tempfile::tempdir().unwrap();
    let cfg = ExportConfig {
        mode: ExportMode::Filesystem {
            dir: tmp.path().to_path_buf(),
        },
        enabled: false,
    };
    let result = export_audit(&pool, &cfg).await.unwrap();
    assert_eq!(result.rows_exported, 0);
    let entries: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(entries.is_empty());
}

/// PR #106 remediation I6/T5: A-14 incremental cursor semantics.
/// Export twice should skip already-exported rows; appending one more
/// row between runs exports only the new row. A regression that dropped
/// the `audit_export_state` UPSERT would re-export the whole chain on
/// every tick, doubling SIEM ingestion cost and breaking the
/// "incremental" claim in the src/audit/export.rs module doc.
#[tokio::test]
async fn incremental_cursor_skips_already_exported_rows() {
    let pool = setup_pool().await;
    let appender = AuditAppender::new_for_tests(pool.clone());
    for i in 0..3 {
        appender
            .append(AuditEvent {
                principal_key: format!("u{i}"),
                principal_type: "user".into(),
                action: AuditAction::AuthSuccess,
                resource_type: None,
                resource_id: None,
                result: "allowed".into(),
                source_ip: None,
                request_id: None,
                metadata: serde_json::json!({"i": i}),
            })
            .await
            .unwrap();
    }
    let tmp = tempfile::tempdir().unwrap();
    let cfg = ExportConfig {
        enabled: true,
        mode: ExportMode::Filesystem {
            dir: tmp.path().to_path_buf(),
        },
    };

    let first = export_audit(&pool, &cfg).await.unwrap();
    assert_eq!(first.rows_exported, 3, "first run exports all seeded rows");
    assert_eq!(first.first_seq, Some(1));
    assert_eq!(first.last_seq, Some(3));

    // Second run on the same data: cursor already at seq=3, no new rows.
    let second = export_audit(&pool, &cfg).await.unwrap();
    assert_eq!(
        second.rows_exported, 0,
        "second run must skip already-exported rows; cursor advance regression"
    );
    assert!(second.first_seq.is_none());
    assert!(second.last_seq.is_none());

    // Append a 4th event, export again: should export only the new row.
    appender
        .append(AuditEvent {
            principal_key: "u3".into(),
            principal_type: "user".into(),
            action: AuditAction::AuthSuccess,
            resource_type: None,
            resource_id: None,
            result: "allowed".into(),
            source_ip: None,
            request_id: None,
            metadata: serde_json::json!({"i": 3}),
        })
        .await
        .unwrap();
    let third = export_audit(&pool, &cfg).await.unwrap();
    assert_eq!(third.rows_exported, 1, "third run exports only the new row");
    assert_eq!(third.first_seq, Some(4));
    assert_eq!(third.last_seq, Some(4));

    // Confirm the cursor is persisted in audit_export_state.
    let stored_seq: i64 = sqlx::query_scalar(
        "SELECT last_exported_seq FROM audit_export_state WHERE export_mode = 'filesystem'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        stored_seq, 4,
        "audit_export_state.last_exported_seq must advance to 4 after 3 exports"
    );
}
