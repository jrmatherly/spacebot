//! Backend integration tests for the admin access-review CSV/JSON endpoint.
//! Covers the two authorization paths plus the format-switch contract:
//!
//! - Non-admin principal: 403
//! - Admin principal, format=csv: 200 + RFC 4180 CSV body
//! - Admin principal, format=json: 200 + JSON array

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt as _;
use spacebot::api::ApiState;
use spacebot::api::test_support::build_test_router_entra;
use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::repository::{upsert_team, upsert_user_from_auth};
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
async fn non_admin_cannot_read_access_review() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user("alice", vec!["SpacebotUser"]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    let app = build_test_router_entra(state);
    let req = Request::builder()
        .uri("/api/admin/access-review?format=csv")
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
async fn admin_gets_csv_report() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user("alice", vec!["SpacebotUser"]);
    let admin = user("admin", vec!["SpacebotAdmin"]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &admin).await.unwrap();
    let team = upsert_team(&pool, "grp-1", "Platform").await.unwrap();
    sqlx::query(
        "INSERT INTO team_memberships (principal_key, team_id, source) VALUES (?, ?, 'token_claim')",
    )
    .bind(alice.principal_key())
    .bind(&team.id)
    .execute(&pool)
    .await
    .unwrap();

    let app = build_test_router_entra(state);
    let req = Request::builder()
        .uri("/api/admin/access-review?format=csv")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", mint_mock_token(&admin)),
        )
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let csv = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        csv.starts_with("principal_key,display_name"),
        "CSV header missing: {csv}"
    );
    assert!(csv.contains("alice"));
    assert!(csv.contains("Platform"));
}

#[tokio::test]
async fn admin_gets_json_report() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user("alice", vec!["SpacebotUser"]);
    let admin = user("admin", vec!["SpacebotAdmin"]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &admin).await.unwrap();
    let team = upsert_team(&pool, "grp-2", "Security").await.unwrap();
    sqlx::query(
        "INSERT INTO team_memberships (principal_key, team_id, source) VALUES (?, ?, 'token_claim')",
    )
    .bind(alice.principal_key())
    .bind(&team.id)
    .execute(&pool)
    .await
    .unwrap();

    let app = build_test_router_entra(state);
    let req = Request::builder()
        .uri("/api/admin/access-review?format=json")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", mint_mock_token(&admin)),
        )
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).expect("valid JSON array");
    let arr = v.as_array().expect("top-level array");
    assert!(!arr.is_empty(), "expected rows for alice + admin");
    let alice_row = arr
        .iter()
        .find(|r| r["principal_key"].as_str() == Some(alice.principal_key().as_str()))
        .expect("alice row present");
    let teams = alice_row["teams"].as_array().expect("teams array");
    assert!(
        teams.iter().any(|t| t.as_str() == Some("Security")),
        "expected Security team in alice's teams list, got {teams:?}",
    );
}
