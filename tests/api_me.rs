//! Integration tests for the consolidated `GET /api/me` endpoint.
//!
//! Verifies the handler's observable contract: authenticated principal
//! gets a 200 with the MeResponse shape; roles + groups + display name
//! fields come from AuthContext; photo data URL is populated from the
//! cached `users.display_photo_b64` row when present; initials are
//! computed when photo is absent. Also covers the non-User
//! principal_type serialization path and the photo-row-with-null-blob
//! edge case.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt as _;
use serde_json::Value;
use spacebot::api::ApiState;
use spacebot::api::test_support::build_test_router_entra;
use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::repository::upsert_user_from_auth;
use spacebot::auth::roles::ROLE_USER;
use spacebot::auth::testing::mint_mock_token;
use std::sync::Arc;
use tower::ServiceExt as _;

fn user_ctx(oid: &str, display_name: Option<&str>, roles: Vec<&str>) -> AuthContext {
    AuthContext {
        principal_type: PrincipalType::User,
        tid: Arc::from("tenant-1"),
        oid: Arc::from(oid),
        roles: roles.into_iter().map(Arc::from).collect(),
        groups: vec![],
        groups_overage: false,
        display_email: Some(Arc::from(format!("{oid}@example.com").as_str())),
        display_name: display_name.map(Arc::from),
    }
}

fn req_me(bearer: &str) -> Request<Body> {
    Request::builder()
        .uri("/api/me")
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap()
}

async fn read_json(res: axum::response::Response) -> Value {
    let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body_bytes).unwrap()
}

#[tokio::test]
async fn returns_initials_when_photo_absent() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice", Some("Alice Example"), vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let res = app.oneshot(req_me(&token)).await.unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = read_json(res).await;
    assert_eq!(body["principal_key"], "tenant-1:alice");
    assert_eq!(body["tid"], "tenant-1");
    assert_eq!(body["oid"], "alice");
    assert_eq!(body["principal_type"], "user");
    assert_eq!(body["display_name"], "Alice Example");
    assert_eq!(body["display_email"], "alice@example.com");
    // Photo absent → data URL null, initials computed from display name.
    assert!(body["display_photo_data_url"].is_null());
    assert_eq!(body["initials"], "AE");
    assert_eq!(body["groups_overage"], false);
}

#[tokio::test]
async fn returns_photo_data_url_when_cached() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice", Some("Alice Example"), vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();

    // Seed the cached photo row. Mimics what the middleware's
    // fire-and-forget sync_user_photo_for_principal would write after
    // Graph's /me/photo/$value 200 response.
    let fake_b64 = "FAKEBASE64PAYLOAD==";
    sqlx::query("UPDATE users SET display_photo_b64 = ?, photo_updated_at = datetime('now') WHERE principal_key = ?")
        .bind(fake_b64)
        .bind(alice.principal_key())
        .execute(&pool)
        .await
        .unwrap();

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let res = app.oneshot(req_me(&token)).await.unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = read_json(res).await;
    assert_eq!(
        body["display_photo_data_url"],
        format!("data:image/jpeg;base64,{fake_b64}")
    );
    // When photo is present, initials must be null so the SPA does
    // not fall back to initials and ignore the photo.
    assert!(body["initials"].is_null());
}

#[tokio::test]
async fn returns_roles_from_auth_context() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice", Some("Alice"), vec![ROLE_USER, "SpacebotAdmin"]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let res = app.oneshot(req_me(&token)).await.unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = read_json(res).await;
    let roles = body["roles"].as_array().unwrap();
    assert_eq!(roles.len(), 2);
    let role_strs: Vec<&str> = roles.iter().map(|r| r.as_str().unwrap()).collect();
    assert!(role_strs.contains(&ROLE_USER));
    assert!(role_strs.contains(&"SpacebotAdmin"));
}

#[tokio::test]
async fn rejects_missing_bearer_with_401() {
    let (state, _pool) = ApiState::new_test_state_with_mock_entra().await;
    let app = build_test_router_entra(state);
    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/me")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn photo_row_present_with_null_blob_falls_back_to_initials() {
    // The 404-from-Graph normal case: upsert_user_from_auth created
    // the users row with display_photo_b64 = NULL (default). This
    // differentiates "row missing" from "row present with null blob".
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice", Some("Alice Example"), vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();

    // Explicitly set display_photo_b64 = NULL + a fresh photo_updated_at
    // to simulate the state the photo-sync middleware leaves after
    // Graph returned 404 (normal for accounts with no photo).
    sqlx::query("UPDATE users SET display_photo_b64 = NULL, photo_updated_at = datetime('now') WHERE principal_key = ?")
        .bind(alice.principal_key())
        .execute(&pool)
        .await
        .unwrap();

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let res = app.oneshot(req_me(&token)).await.unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = read_json(res).await;
    assert!(body["display_photo_data_url"].is_null());
    // Initials must still compute from display_name even when the row
    // exists (just with null blob). A regression that checks "row
    // exists" instead of "b64 is Some" would serve data:image/jpeg,null.
    assert_eq!(body["initials"], "AE");
}

#[tokio::test]
async fn service_principal_serializes_as_snake_case() {
    // PrincipalType::ServicePrincipal must serialize as
    // "service_principal" on the wire, not "serviceprincipal" or
    // "ServicePrincipal". The enum's `#[serde(rename_all = "snake_case")]`
    // gates this; a refactor that falls back to `format!("{:?}")`
    // would ship the wrong value.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let sp = AuthContext {
        principal_type: PrincipalType::ServicePrincipal,
        tid: Arc::from("tenant-1"),
        oid: Arc::from("app-client-1"),
        roles: vec![Arc::from("SpacebotService")],
        groups: vec![],
        groups_overage: false,
        display_email: None,
        display_name: Some(Arc::from("Builder Service")),
    };
    upsert_user_from_auth(&pool, &sp).await.unwrap();

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&sp);
    let res = app.oneshot(req_me(&token)).await.unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = read_json(res).await;
    assert_eq!(body["principal_type"], "service_principal");
}
