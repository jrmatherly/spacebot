//! Integration tests for `GET /api/teams`.
//!
//! Phase 7 PR 2 Task 7.7.5. The endpoint lists active teams for the SPA's
//! ShareResourceModal selector. Authenticated-only (any signed-in user);
//! admin-only listing ships separately under /admin/teams in PR 5.
//!
//! Coverage matrix:
//!   - `authenticated_user_gets_active_teams_sorted_by_display_name`
//!   - `unauthenticated_returns_401` prevents a silent loss of auth
//!     gating from masking as "empty list returned".
//!   - `archived_teams_are_filtered` proves the SQL filter, not just
//!     the repo helper unit test.
//!   - `empty_teams_table_returns_empty_array`
//!   - `user_without_spacebot_user_role_is_forbidden`
//!   - `service_principal_without_spacebot_user_role_is_forbidden`
//!   - `admin_with_spacebot_user_role_grants_access`

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use spacebot::api::test_support::build_test_router_entra;
use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::repository::{upsert_team, upsert_user_from_auth};
use spacebot::auth::testing::mint_mock_token;
use std::sync::Arc;
use tower::ServiceExt as _;

fn user(oid: &str) -> AuthContext {
    AuthContext {
        principal_type: PrincipalType::User,
        tid: Arc::from("t1"),
        oid: Arc::from(oid),
        roles: vec![Arc::from("SpacebotUser")],
        groups: vec![],
        groups_overage: false,
        display_email: None,
        display_name: None,
    }
}

fn user_with_roles(oid: &str, roles: Vec<&str>) -> AuthContext {
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

fn service_principal(oid: &str, roles: Vec<&str>) -> AuthContext {
    AuthContext {
        principal_type: PrincipalType::ServicePrincipal,
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
async fn authenticated_user_gets_active_teams_sorted_by_display_name() {
    let (state, pool) = spacebot::api::ApiState::new_test_state_with_mock_entra().await;
    let alice = user("alice");
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    // Create in non-sorted order to confirm the endpoint orders by display_name.
    upsert_team(&pool, "grp-z", "Zephyr").await.unwrap();
    upsert_team(&pool, "grp-a", "Atlas").await.unwrap();
    upsert_team(&pool, "grp-m", "Meridian").await.unwrap();
    let app = build_test_router_entra(state);

    let token = mint_mock_token(&alice);
    let req = Request::builder()
        .method("GET")
        .uri("/api/teams")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(res.into_body(), 64 * 1024)
        .await
        .unwrap();
    let teams: Vec<serde_json::Value> = serde_json::from_slice(&body_bytes).unwrap();
    let names: Vec<&str> = teams
        .iter()
        .map(|t| t["display_name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        vec!["Atlas", "Meridian", "Zephyr"],
        "teams must be sorted by display_name ASC"
    );
    // Shape check: only id + display_name crossed the wire.
    assert!(teams[0].get("id").is_some(), "id field present");
    assert!(
        teams[0].get("status").is_none(),
        "status must not leak to the SPA (filtered to active in SQL)"
    );
    assert!(
        teams[0].get("created_at").is_none(),
        "timestamps must not leak"
    );
}

#[tokio::test]
async fn unauthenticated_returns_401() {
    // Structural auth guard: if a router refactor drops the auth
    // layer, an empty teams table would quietly return 200 []. This
    // omits the Authorization header so the missing auth surfaces as
    // 401 explicitly.
    let (state, _pool) = spacebot::api::ApiState::new_test_state_with_mock_entra().await;
    let app = build_test_router_entra(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/teams")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::UNAUTHORIZED,
        "missing Authorization header must fail as 401, not 200 with empty array"
    );
}

#[tokio::test]
async fn archived_teams_are_filtered() {
    let (state, pool) = spacebot::api::ApiState::new_test_state_with_mock_entra().await;
    let alice = user("alice");
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    let active = upsert_team(&pool, "grp-active", "Active Team")
        .await
        .unwrap();
    let archived = upsert_team(&pool, "grp-archived", "Archived Team")
        .await
        .unwrap();
    // Archive one team directly. The CHECK constraint on `teams.status`
    // accepts only {'active', 'archived'}; a future graph-sync sweep will
    // UPDATE teams SET status = 'archived' when Graph removes a group.
    sqlx::query("UPDATE teams SET status = 'archived' WHERE id = ?")
        .bind(&archived.id)
        .execute(&pool)
        .await
        .unwrap();
    let app = build_test_router_entra(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/teams")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", mint_mock_token(&alice)),
        )
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(res.into_body(), 64 * 1024)
        .await
        .unwrap();
    let teams: Vec<serde_json::Value> = serde_json::from_slice(&body_bytes).unwrap();
    let ids: Vec<&str> = teams.iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert!(
        ids.contains(&active.id.as_str()),
        "active team must be listed"
    );
    assert!(
        !ids.contains(&archived.id.as_str()),
        "archived team must not be listed"
    );
}

#[tokio::test]
async fn empty_teams_table_returns_empty_array() {
    let (state, pool) = spacebot::api::ApiState::new_test_state_with_mock_entra().await;
    let alice = user("alice");
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    let app = build_test_router_entra(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/teams")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", mint_mock_token(&alice)),
        )
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(res.into_body(), 64 * 1024)
        .await
        .unwrap();
    let teams: Vec<serde_json::Value> = serde_json::from_slice(&body_bytes).unwrap();
    assert!(teams.is_empty(), "zero teams → []");
}

#[tokio::test]
async fn user_without_spacebot_user_role_is_forbidden() {
    // Role gate pinning: a User principal missing the `SpacebotUser`
    // role must be denied with 403. Prevents a future refactor from
    // silently removing the `require_role` call.
    let (state, pool) = spacebot::api::ApiState::new_test_state_with_mock_entra().await;
    let bob = user_with_roles("bob", vec![]);
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    let app = build_test_router_entra(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/teams")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", mint_mock_token(&bob)),
        )
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "role-less User must fail as 403, not 200"
    );
}

#[tokio::test]
async fn service_principal_without_spacebot_user_role_is_forbidden() {
    // Service principals carrying only `SpacebotService` must not
    // enumerate team names. If an M2M caller genuinely needs the list,
    // it should hold `SpacebotUser` as well.
    let (state, pool) = spacebot::api::ApiState::new_test_state_with_mock_entra().await;
    let svc = service_principal("svc", vec!["SpacebotService"]);
    upsert_user_from_auth(&pool, &svc).await.unwrap();
    let app = build_test_router_entra(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/teams")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", mint_mock_token(&svc)),
        )
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "SpacebotService-only principal must fail as 403"
    );
}

#[tokio::test]
async fn same_display_name_sort_is_stable_by_id() {
    // Guard against a nondeterministic UI team-selector under the
    // collision case (two Entra groups renamed to the same display
    // name). The ORDER BY clause adds `id` as tiebreaker so SQLite
    // returns the rows in a deterministic order across restarts.
    let (state, pool) = spacebot::api::ApiState::new_test_state_with_mock_entra().await;
    let alice = user("alice");
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    // Two teams with identical display_name; tiebreaker is `id`,
    // which `upsert_team` builds from `external_id`.
    upsert_team(&pool, "grp-b", "Platform").await.unwrap();
    upsert_team(&pool, "grp-a", "Platform").await.unwrap();
    let app = build_test_router_entra(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/teams")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", mint_mock_token(&alice)),
        )
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), 64 * 1024)
        .await
        .unwrap();
    let teams: Vec<serde_json::Value> = serde_json::from_slice(&body_bytes).unwrap();
    let ids: Vec<&str> = teams.iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert_eq!(
        ids,
        vec!["team-grp-a", "team-grp-b"],
        "identical display_name must order deterministically by id"
    );
}

#[tokio::test]
async fn admin_with_spacebot_user_role_grants_access() {
    // A User holding both SpacebotUser and SpacebotAdmin passes the
    // role gate. Admin is an additive role (no superset-of-User
    // behavior on `require_role`), so operators assigning only
    // SpacebotAdmin would be blocked. That matches the existing
    // `require_role` contract in `src/auth/roles.rs`.
    let (state, pool) = spacebot::api::ApiState::new_test_state_with_mock_entra().await;
    let admin = user_with_roles("admin", vec!["SpacebotUser", "SpacebotAdmin"]);
    upsert_user_from_auth(&pool, &admin).await.unwrap();
    let app = build_test_router_entra(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/teams")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", mint_mock_token(&admin)),
        )
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
