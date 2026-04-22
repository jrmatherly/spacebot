//! Phase 5 Task 5.8 — admin-only audit read endpoint authz tests.
//!
//! Covers:
//! - non-admin principal gets 403 from GET /api/admin/audit
//! - admin principal gets 200 + NDJSON body containing seeded events
//! - admin gets CSV body when Accept: text/csv (PR #106 remediation I3)
//! - /api/admin/audit/verify returns { valid: true } on a clean chain
//!   (PR #106 remediation I2/G3)
//! - /api/admin/audit/verify returns { valid: false, first_mismatch_seq }
//!   after direct SQL tamper (PR #106 remediation I2/G3)

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt as _;
use spacebot::api::ApiState;
use spacebot::api::test_support::build_test_router_entra;
use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::testing::mint_mock_token;
use std::sync::Arc;
use tower::ServiceExt as _;

fn user(oid: &str, roles: Vec<&str>) -> AuthContext {
    AuthContext {
        principal_type: PrincipalType::User,
        tid: Arc::from("t1"),
        oid: Arc::from(oid),
        roles: roles.into_iter().map(Arc::from).collect(),
        groups: vec![],
        groups_overage: false,
        display_email: None,
        display_name: None,
    }
}

#[tokio::test]
async fn non_admin_gets_403() {
    let (state, _pool) = ApiState::new_test_state_with_mock_entra().await;
    let app = build_test_router_entra(state);
    let token = mint_mock_token(&user("alice", vec!["SpacebotUser"]));
    let req = Request::builder()
        .uri("/api/admin/audit?limit=10")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_gets_ndjson_list() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    // Seed a couple of events via AuditAppender.
    let appender = spacebot::audit::AuditAppender::new_for_tests(pool.clone());
    use spacebot::audit::{AuditAction, AuditEvent};
    appender
        .append(AuditEvent {
            principal_key: "alice".into(),
            principal_type: "user".into(),
            action: AuditAction::AuthSuccess,
            resource_type: None,
            resource_id: None,
            result: "allowed".into(),
            source_ip: None,
            request_id: None,
            metadata: serde_json::Value::Null,
        })
        .await
        .unwrap();

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&user("admin", vec!["SpacebotAdmin"]));
    let req = Request::builder()
        .uri("/api/admin/audit?limit=10")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::ACCEPT, "application/x-ndjson")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("auth_success"));
    assert!(body_str.contains("alice"));
}

/// PR #106 remediation I3: the CSV Accept branch at src/api/audit.rs is a
/// distinct code path (separate content-type + distinct serialization). A
/// broken column order or missing header line would ship undetected under
/// the NDJSON-only test.
#[tokio::test]
async fn admin_gets_csv_when_accept_csv() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let appender = spacebot::audit::AuditAppender::new_for_tests(pool.clone());
    use spacebot::audit::{AuditAction, AuditEvent};
    appender
        .append(AuditEvent {
            principal_key: "alice".into(),
            principal_type: "user".into(),
            action: AuditAction::AuthSuccess,
            resource_type: Some("memory".into()),
            resource_id: Some("mem-1".into()),
            result: "allowed".into(),
            source_ip: None,
            request_id: None,
            metadata: serde_json::Value::Null,
        })
        .await
        .unwrap();

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&user("admin", vec!["SpacebotAdmin"]));
    let req = Request::builder()
        .uri("/api/admin/audit?limit=10")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::ACCEPT, "text/csv")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers()
            .get(header::CONTENT_TYPE)
            .map(|v| v.as_bytes()),
        Some(&b"text/csv"[..]),
        "Content-Type should be text/csv when Accept: text/csv"
    );
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    // Header row first, then data row. Exact column set is the
    // contract: seq, timestamp, principal_key, action, resource_type,
    // resource_id, result.
    assert!(
        body_str.starts_with("seq,timestamp,principal_key,action,resource_type,resource_id,result"),
        "CSV must lead with the canonical header row; got: {}",
        body_str.lines().next().unwrap_or("<empty>")
    );
    assert!(body_str.contains("auth_success"));
    assert!(body_str.contains("alice"));
    assert!(body_str.contains("memory"));
    assert!(body_str.contains("mem-1"));
}

/// PR #106 remediation I2/G3: the /api/admin/audit/verify handler was
/// untested at the HTTP surface. Primitive verify_chain() is covered in
/// audit_chain.rs; this closes the wire.
#[tokio::test]
async fn verify_returns_valid_for_clean_chain() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let appender = spacebot::audit::AuditAppender::new_for_tests(pool.clone());
    use spacebot::audit::{AuditAction, AuditEvent};
    for _ in 0..3 {
        appender
            .append(AuditEvent {
                principal_key: "alice".into(),
                principal_type: "user".into(),
                action: AuditAction::AuthSuccess,
                resource_type: None,
                resource_id: None,
                result: "allowed".into(),
                source_ip: None,
                request_id: None,
                metadata: serde_json::Value::Null,
            })
            .await
            .unwrap();
    }

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&user("admin", vec!["SpacebotAdmin"]));
    let req = Request::builder()
        .uri("/api/admin/audit/verify")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&body_str).unwrap();
    assert_eq!(v["valid"], serde_json::Value::Bool(true));
    assert!(v["first_mismatch_seq"].is_null());
    assert_eq!(v["total_rows"].as_i64(), Some(3));
}

/// PR #106 remediation I2/G3: tamper-path HTTP test. Mirrors the
/// chain_verify_detects_tamper unit test through the admin endpoint so a
/// regression that breaks the handler-to-verify_chain wiring is caught.
#[tokio::test]
async fn verify_reports_mismatch_for_tampered_chain() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let appender = spacebot::audit::AuditAppender::new_for_tests(pool.clone());
    use spacebot::audit::{AuditAction, AuditEvent};
    for _ in 0..2 {
        appender
            .append(AuditEvent {
                principal_key: "alice".into(),
                principal_type: "user".into(),
                action: AuditAction::AuthSuccess,
                resource_type: None,
                resource_id: None,
                result: "allowed".into(),
                source_ip: None,
                request_id: None,
                metadata: serde_json::Value::Null,
            })
            .await
            .unwrap();
    }
    // Tamper: change the action at seq=1 directly.
    sqlx::query("UPDATE audit_events SET action = 'forged' WHERE seq = 1")
        .execute(&pool)
        .await
        .unwrap();

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&user("admin", vec!["SpacebotAdmin"]));
    let req = Request::builder()
        .uri("/api/admin/audit/verify")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["valid"], serde_json::Value::Bool(false));
    assert_eq!(v["first_mismatch_seq"].as_i64(), Some(1));
    assert_eq!(v["total_rows"].as_i64(), Some(2));
}

/// PR #106 remediation I2/G3: non-admin must be rejected from /verify too.
#[tokio::test]
async fn non_admin_gets_403_on_verify() {
    let (state, _pool) = ApiState::new_test_state_with_mock_entra().await;
    let app = build_test_router_entra(state);
    let token = mint_mock_token(&user("alice", vec!["SpacebotUser"]));
    let req = Request::builder()
        .uri("/api/admin/audit/verify")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}
