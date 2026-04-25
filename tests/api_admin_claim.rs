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
}
