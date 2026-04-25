//! Backend integration tests for the admin orphans endpoint. Covers the
//! two authorization paths plus AdminRead audit-emission verification.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt as _;
use spacebot::api::ApiState;
use spacebot::api::test_support::build_test_router_entra;
use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::repository::upsert_user_from_auth;
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
        display_email: Some(Arc::from(format!("{oid}@example.com").as_str())),
        display_name: Some(Arc::from(format!("User {oid}").as_str())),
    }
}

#[tokio::test]
async fn non_admin_cannot_list_orphans() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user("alice", vec!["SpacebotUser"]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    let app = build_test_router_entra(state);
    let req = Request::builder()
        .uri("/api/admin/orphans")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", mint_mock_token(&alice)),
        )
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_lists_orphans_and_audit_event_landed() {
    // I3: pin AdminRead audit emission for the orphans endpoint.
    // The sweep itself returns an empty list (no agent DBs in the
    // test fixture), but the audit row must still be persisted so
    // SOC 2 evidence reflects every cross-agent scan attempt.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let admin = user("admin_orphans", vec!["SpacebotAdmin"]);
    upsert_user_from_auth(&pool, &admin).await.unwrap();

    // SAFETY: discover_agent_db_paths reads SPACEBOT_DIR; clear it so
    // the test result doesn't depend on the host's environment.
    unsafe {
        std::env::remove_var("SPACEBOT_DIR");
    }

    let app = build_test_router_entra(state);
    let req = Request::builder()
        .uri("/api/admin/orphans")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", mint_mock_token(&admin)),
        )
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).expect("valid JSON");
    assert_eq!(v["agent_dbs_scanned"].as_u64(), Some(0));
    assert!(v["orphans"].as_array().is_some_and(|a| a.is_empty()));

    // AdminRead must be persisted for the orphans scan, with metadata
    // recording the scope of the cross-agent read.
    type AuditRow = (String, String, Option<String>, Option<String>, String);
    let mut audit_row: Option<AuditRow> = None;
    for _ in 0..5 {
        if let Ok(row) = sqlx::query_as::<_, AuditRow>(
            "SELECT principal_key, action, resource_type, resource_id, metadata_json \
             FROM audit_events \
             WHERE action = ? AND resource_type = ? \
             ORDER BY seq DESC LIMIT 1",
        )
        .bind("admin_read")
        .bind("orphans")
        .fetch_one(&pool)
        .await
        {
            audit_row = Some(row);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let (audited_principal, audited_action, audited_rt, _, audited_metadata) =
        audit_row.expect("admin_read/orphans audit event was not appended");
    assert_eq!(audited_principal, admin.principal_key());
    assert_eq!(audited_action, "admin_read");
    assert_eq!(audited_rt.as_deref(), Some("orphans"));
    let metadata: serde_json::Value =
        serde_json::from_str(&audited_metadata).expect("metadata_json is valid JSON");
    assert!(
        metadata["agent_dbs_scanned"].as_u64().is_some(),
        "audit metadata must record agent_dbs_scanned: {metadata}",
    );
    assert!(
        metadata["orphan_count"].as_u64().is_some(),
        "audit metadata must record orphan_count: {metadata}",
    );
}
