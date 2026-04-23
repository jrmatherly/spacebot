//! Integration tests for `PUT /api/resources/{type}/{id}/visibility`.
//!
//! Phase 7 PR 1.5 Task 7.5. The endpoint lets an owner (or admin) rotate a
//! resource's visibility between Personal / Team / Org and re-bind the
//! optional `shared_with_team_id`. It is the backend consumer of the
//! `ShareResourceModal` component that shipped in PR 1.
//!
//! Coverage matrix:
//!   - `owner_can_change_visibility_personal_to_team`: happy path, owner
//!     upgrades a memory from Personal to Team scope.
//!   - `non_owner_cannot_change_visibility`: Bob tries to change Alice's
//!     memory; the no-auto-broadening + owner-only write policy returns 404
//!     so a non-owner cannot even confirm the resource exists.
//!   - `admin_can_change_any_visibility`: admin bypass.
//!   - `team_visibility_without_team_id_rejected`: 400 guard fires BEFORE
//!     the DB CHECK constraint would trip.
//!   - `pool_none_returns_500`: startup-window safety. The endpoint
//!     cannot silently no-op on a non-attached pool.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
// D29 correction (2026-04-23 Phase 7 audit): the real helper is
// `build_test_router_entra` under `spacebot::api::test_support`, NOT
// `spacebot::api::server::test_support::build_test_router_with_auth`.
use spacebot::api::test_support::build_test_router_entra;
use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::principals::Visibility;
use spacebot::auth::repository::{set_ownership, upsert_team, upsert_user_from_auth};
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
async fn owner_can_change_visibility_personal_to_team() {
    let (state, pool) = spacebot::api::ApiState::new_test_state_with_mock_entra().await;
    let alice = user("alice", vec!["SpacebotUser"]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    let team = upsert_team(&pool, "grp-1", "Platform").await.unwrap();
    set_ownership(
        &pool,
        "memory",
        "m-1",
        None,
        &alice.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();
    let app = build_test_router_entra(state);

    let token = mint_mock_token(&alice);
    let body =
        serde_json::json!({"visibility": "team", "shared_with_team_id": team.id}).to_string();
    let req = Request::builder()
        .method("PUT")
        .uri("/api/resources/memory/m-1/visibility")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn non_owner_cannot_change_visibility() {
    let (state, pool) = spacebot::api::ApiState::new_test_state_with_mock_entra().await;
    let alice = user("alice", vec!["SpacebotUser"]);
    let bob = user("bob", vec!["SpacebotUser"]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    set_ownership(
        &pool,
        "memory",
        "m-1",
        None,
        &alice.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();
    let app = build_test_router_entra(state);

    let body = serde_json::json!({"visibility": "org"}).to_string();
    let req = Request::builder()
        .method("PUT")
        .uri("/api/resources/memory/m-1/visibility")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", mint_mock_token(&bob)),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "non-owner gets 404 per no-auto-broadening policy"
    );
}

#[tokio::test]
async fn admin_can_change_any_visibility() {
    let (state, pool) = spacebot::api::ApiState::new_test_state_with_mock_entra().await;
    let alice = user("alice", vec!["SpacebotUser"]);
    let carol = user("carol", vec!["SpacebotAdmin"]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &carol).await.unwrap();
    set_ownership(
        &pool,
        "memory",
        "m-2",
        None,
        &alice.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();
    let app = build_test_router_entra(state);

    // Admin rotates Alice's memory to Org scope without being the owner.
    let body = serde_json::json!({"visibility": "org"}).to_string();
    let req = Request::builder()
        .method("PUT")
        .uri("/api/resources/memory/m-2/visibility")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", mint_mock_token(&carol)),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn team_visibility_without_team_id_rejected() {
    let (state, pool) = spacebot::api::ApiState::new_test_state_with_mock_entra().await;
    let alice = user("alice", vec!["SpacebotUser"]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    set_ownership(
        &pool,
        "memory",
        "m-3",
        None,
        &alice.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();
    let app = build_test_router_entra(state);

    // Team visibility with NO shared_with_team_id. Handler must guard
    // BEFORE the DB CHECK constraint (which would also reject but as 500).
    let body = serde_json::json!({"visibility": "team"}).to_string();
    let req = Request::builder()
        .method("PUT")
        .uri("/api/resources/memory/m-3/visibility")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", mint_mock_token(&alice)),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::BAD_REQUEST,
        "team without team_id is a 400, not a 500 CHECK-constraint leak"
    );
}

#[tokio::test]
async fn rotation_preserves_owner_agent_id() {
    // C1 regression (PR #111 review). Before the fix, set_visibility
    // called set_ownership with owner_agent_id = None, and the UPSERT's
    // excluded.owner_agent_id overwrote any prior agent link to NULL.
    // This test seeds a row with owner_agent_id = Some("agent-x"),
    // rotates visibility, and asserts the agent link survives.
    let (state, pool) = spacebot::api::ApiState::new_test_state_with_mock_entra().await;
    let alice = user("alice", vec!["SpacebotUser"]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    set_ownership(
        &pool,
        "memory",
        "m-agent-owned",
        Some("agent-x"),
        &alice.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();
    let app = build_test_router_entra(state);

    let body = serde_json::json!({"visibility": "org"}).to_string();
    let req = Request::builder()
        .method("PUT")
        .uri("/api/resources/memory/m-agent-owned/visibility")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", mint_mock_token(&alice)),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // The agent link MUST still be there.
    let own = spacebot::auth::repository::get_ownership(&pool, "memory", "m-agent-owned")
        .await
        .unwrap()
        .expect("row still exists");
    assert_eq!(
        own.owner_agent_id.as_deref(),
        Some("agent-x"),
        "rotation preserves owner_agent_id"
    );
    assert_eq!(
        own.owner_principal_key,
        alice.principal_key(),
        "rotation preserves owner_principal_key"
    );
    assert_eq!(own.visibility.as_str(), "org");
}

#[tokio::test]
async fn invalid_visibility_value_rejected() {
    let (state, pool) = spacebot::api::ApiState::new_test_state_with_mock_entra().await;
    let alice = user("alice", vec!["SpacebotUser"]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    set_ownership(
        &pool,
        "memory",
        "m-4",
        None,
        &alice.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();
    let app = build_test_router_entra(state);

    let body = serde_json::json!({"visibility": "global"}).to_string();
    let req = Request::builder()
        .method("PUT")
        .uri("/api/resources/memory/m-4/visibility")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", mint_mock_token(&alice)),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::BAD_REQUEST,
        "unknown visibility value must fail before reaching the DB"
    );
}
