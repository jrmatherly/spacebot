//! Chain-integrity tests. Every row's `prev_hash` matches the prior row's
//! `row_hash`; `row_hash` is deterministic given canonical event bytes.

use spacebot::audit::appender::AuditAppender;
use spacebot::audit::types::{AuditAction, AuditEvent};
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

fn sample_event(principal: &str, action: AuditAction) -> AuditEvent {
    AuditEvent {
        principal_key: principal.into(),
        principal_type: "user".into(),
        action,
        resource_type: Some("memory".into()),
        resource_id: Some("mem-1".into()),
        result: "allowed".into(),
        source_ip: Some("10.0.0.1".into()),
        request_id: Some("req-abc".into()),
        metadata: serde_json::json!({"kind": "test"}),
    }
}

#[tokio::test]
async fn first_row_uses_zero_prev_hash() {
    let pool = setup_pool().await;
    let appender = AuditAppender::new_for_tests(pool.clone());
    let row = appender
        .append(sample_event("u1", AuditAction::AuthSuccess))
        .await
        .unwrap();
    assert_eq!(row.seq, 1);
    assert_eq!(
        row.prev_hash,
        "0".repeat(64),
        "first row's prev_hash must be 64 zeros"
    );
    assert!(!row.row_hash.is_empty());
    assert_eq!(row.row_hash.len(), 64, "sha256 hex = 64 chars");
}

#[tokio::test]
async fn chain_is_linked() {
    let pool = setup_pool().await;
    let appender = AuditAppender::new_for_tests(pool.clone());
    let a = appender
        .append(sample_event("u1", AuditAction::AuthSuccess))
        .await
        .unwrap();
    let b = appender
        .append(sample_event("u1", AuditAction::ResourceRead))
        .await
        .unwrap();
    let c = appender
        .append(sample_event("u2", AuditAction::AuthFailure))
        .await
        .unwrap();

    assert_eq!(
        b.prev_hash, a.row_hash,
        "row B.prev_hash must equal row A.row_hash"
    );
    assert_eq!(
        c.prev_hash, b.row_hash,
        "row C.prev_hash must equal row B.row_hash"
    );
    assert_eq!(a.seq, 1);
    assert_eq!(b.seq, 2);
    assert_eq!(c.seq, 3);
}

#[tokio::test]
async fn row_hash_is_deterministic_for_same_event() {
    use spacebot::audit::types::canonical_bytes;
    let ev_1 = sample_event("u1", AuditAction::AuthSuccess);
    let ev_2 = sample_event("u1", AuditAction::AuthSuccess);
    let h1 = canonical_bytes(&ev_1, 1, "2026-04-20T00:00:00.000Z", &"0".repeat(64));
    let h2 = canonical_bytes(&ev_2, 1, "2026-04-20T00:00:00.000Z", &"0".repeat(64));
    assert_eq!(
        h1, h2,
        "canonical bytes must be deterministic for equal inputs"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_appends_do_not_corrupt_chain() {
    let pool = setup_pool().await;
    let appender = std::sync::Arc::new(AuditAppender::new_for_tests(pool.clone()));

    let mut handles = Vec::new();
    for i in 0..20 {
        let a = appender.clone();
        handles.push(tokio::spawn(async move {
            a.append(sample_event(&format!("u{i}"), AuditAction::AuthSuccess))
                .await
        }));
    }
    for h in handles {
        h.await.unwrap().unwrap();
    }

    // Verify the full chain: every row's prev_hash matches the prior row.
    let rows: Vec<(i64, String, String)> =
        sqlx::query_as("SELECT seq, prev_hash, row_hash FROM audit_events ORDER BY seq")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(rows.len(), 20);
    assert_eq!(rows[0].1, "0".repeat(64));
    for i in 1..rows.len() {
        assert_eq!(
            rows[i].1,
            rows[i - 1].2,
            "chain break at seq {}: prev_hash != prior row_hash",
            rows[i].0
        );
    }
}

#[tokio::test]
async fn chain_verify_detects_tamper() {
    let pool = setup_pool().await;
    let appender = AuditAppender::new_for_tests(pool.clone());
    appender
        .append(sample_event("u1", AuditAction::AuthSuccess))
        .await
        .unwrap();
    appender
        .append(sample_event("u1", AuditAction::ResourceRead))
        .await
        .unwrap();

    // Tamper: change the action in row 1 directly.
    sqlx::query("UPDATE audit_events SET action = 'forged' WHERE seq = 1")
        .execute(&pool)
        .await
        .unwrap();

    let result = appender.verify_chain().await.unwrap();
    assert!(!result.valid, "verify_chain must detect tampering");
    assert_eq!(result.first_mismatch_seq, Some(1));
}
