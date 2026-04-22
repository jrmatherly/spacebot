//! Phase 5 Task 5.8 — admin-only audit read endpoint authz tests.
//!
//! Covers:
//! - non-admin principal gets 403 from GET /api/admin/audit
//! - admin principal gets 200 + NDJSON body containing seeded events

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
