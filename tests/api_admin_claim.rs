//! Backend integration tests for the admin claim-resource endpoint
//! (Phase 9). Covers the two authorization paths:
//!
//! - Non-admin principal → 403
//! - Admin principal     → 200 (and a resource_ownership row is written)

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
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
async fn non_admin_cannot_claim() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user("alice", vec!["SpacebotUser"]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();

    let app = build_test_router_entra(state);
    let body = serde_json::json!({
        "resource_type": "memory",
        "resource_id": "m-orphan",
        "owner_principal_key": alice.principal_key(),
        "visibility": "personal",
    })
    .to_string();
    let token = mint_mock_token(&alice);
    let req = Request::builder()
        .method("POST")
        .uri("/api/admin/claim-resource")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_can_claim() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user("alice", vec!["SpacebotUser"]);
    let admin = user("admin", vec!["SpacebotAdmin"]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &admin).await.unwrap();

    let app = build_test_router_entra(state);
    let alice_key = alice.principal_key();
    let body = serde_json::json!({
        "resource_type": "memory",
        "resource_id": "m-orphan",
        "owner_principal_key": alice_key,
        "visibility": "personal",
    })
    .to_string();
    let token = mint_mock_token(&admin);
    let req = Request::builder()
        .method("POST")
        .uri("/api/admin/claim-resource")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify the row landed.
    let row: (String, String) = sqlx::query_as(
        "SELECT resource_type, owner_principal_key FROM resource_ownership \
         WHERE resource_type = ? AND resource_id = ?",
    )
    .bind("memory")
    .bind("m-orphan")
    .fetch_one(&pool)
    .await
    .expect("ownership row written");
    assert_eq!(row.0, "memory");
    assert_eq!(row.1, alice.principal_key());

    // Audit emission: the handler spawns the append fire-and-forget. Poll
    // briefly for the row so the spawn has a chance to land before we
    // assert. Five 50ms ticks is a very generous bound for an in-memory
    // SQLite write; flakes here would point at a regression in the
    // append-task path, not test slack.
    let mut audit_row: Option<(String, String, Option<String>, Option<String>, String)> = None;
    for _ in 0..5 {
        if let Ok(row) =
            sqlx::query_as::<_, (String, String, Option<String>, Option<String>, String)>(
                "SELECT principal_key, action, resource_type, resource_id, metadata_json \
             FROM audit_events WHERE action = ? ORDER BY seq DESC LIMIT 1",
            )
            .bind("admin_claim_resource")
            .fetch_one(&pool)
            .await
        {
            audit_row = Some(row);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let (audited_principal, audited_action, audited_rt, audited_rid, audited_metadata) =
        audit_row.expect("admin_claim_resource audit event was not appended");
    assert_eq!(audited_principal, admin.principal_key());
    assert_eq!(audited_action, "admin_claim_resource");
    assert_eq!(audited_rt.as_deref(), Some("memory"));
    assert_eq!(audited_rid.as_deref(), Some("m-orphan"));
    let metadata: serde_json::Value =
        serde_json::from_str(&audited_metadata).expect("metadata_json is valid JSON");
    assert_eq!(
        metadata["claimed_for"],
        serde_json::json!(alice.principal_key())
    );
}

#[tokio::test]
async fn admin_can_claim_non_memory_resource_type() {
    // Phase 9 review S10: the handler is resource_type-agnostic by design;
    // exercise a non-`memory` type to pin that the role gate and ownership
    // write don't accidentally couple to a specific resource_type string.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let bob = user("bob", vec!["SpacebotUser"]);
    let admin = user("admin2", vec!["SpacebotAdmin"]);
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    upsert_user_from_auth(&pool, &admin).await.unwrap();

    let app = build_test_router_entra(state);
    let body = serde_json::json!({
        "resource_type": "agent",
        "resource_id": "a-orphan",
        "owner_principal_key": bob.principal_key(),
        "visibility": "personal",
    })
    .to_string();
    let token = mint_mock_token(&admin);
    let req = Request::builder()
        .method("POST")
        .uri("/api/admin/claim-resource")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let row: (String, String) = sqlx::query_as(
        "SELECT resource_type, owner_principal_key FROM resource_ownership \
         WHERE resource_type = ? AND resource_id = ?",
    )
    .bind("agent")
    .bind("a-orphan")
    .fetch_one(&pool)
    .await
    .expect("ownership row written for resource_type=agent");
    assert_eq!(row.0, "agent");
    assert_eq!(row.1, bob.principal_key());
}

#[tokio::test]
async fn rejects_unknown_field_in_request_body() {
    // Phase 9 review I1: `#[serde(deny_unknown_fields)]` means a typo'd
    // field deserialization 422s rather than silently dropping. Pins
    // the contract so a future refactor can't regress to silent-drop.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let admin = user("admin3", vec!["SpacebotAdmin"]);
    upsert_user_from_auth(&pool, &admin).await.unwrap();

    let app = build_test_router_entra(state);
    // `team_id` is the misspelling; the field is `shared_with_team_id`.
    let body = serde_json::json!({
        "resource_type": "memory",
        "resource_id": "m-typo",
        "owner_principal_key": admin.principal_key(),
        "visibility": "personal",
        "team_id": "t1",
    })
    .to_string();
    let token = mint_mock_token(&admin);
    let req = Request::builder()
        .method("POST")
        .uri("/api/admin/claim-resource")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    // axum's Json extractor maps deserialization failure to 422.
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
