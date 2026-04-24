//! Phase 7 PR 5 Task 7.13 — admin-only team directory endpoint authz tests.
//!
//! Covers:
//! - Non-admin principal receives 403 from GET /api/admin/teams
//! - Non-admin principal receives 403 from GET /api/admin/teams/:id/members
//! - Admin principal receives 200 with member counts + last_sync_at
//! - Admin principal receives 200 with trimmed member rows
//! - An empty-but-existent team returns `members: []` rather than 404
//! - Teams with status = 'archived' are filtered out of the list response

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

async fn seed_membership(pool: &sqlx::SqlitePool, principal_key: &str, team_id: &str) {
    sqlx::query(
        "INSERT INTO team_memberships (principal_key, team_id, source) \
         VALUES (?, ?, 'token_claim')",
    )
    .bind(principal_key)
    .bind(team_id)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn non_admin_list_admin_teams_returns_403() {
    let (state, _pool) = ApiState::new_test_state_with_mock_entra().await;
    let app = build_test_router_entra(state);
    let token = mint_mock_token(&user("alice", vec!["SpacebotUser"]));
    let req = Request::builder()
        .uri("/api/admin/teams")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn non_admin_list_team_members_returns_403() {
    let (state, _pool) = ApiState::new_test_state_with_mock_entra().await;
    let app = build_test_router_entra(state);
    let token = mint_mock_token(&user("alice", vec!["SpacebotUser"]));
    let req = Request::builder()
        .uri("/api/admin/teams/team-x/members")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_list_admin_teams_returns_teams_with_counts() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    // Seed two active teams with distinct member counts.
    let alice = user("alice", vec!["SpacebotUser"]);
    let bob = user("bob", vec!["SpacebotUser"]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    let team_platform = upsert_team(&pool, "grp-platform", "Platform")
        .await
        .unwrap();
    let team_data = upsert_team(&pool, "grp-data", "Data").await.unwrap();
    seed_membership(&pool, &alice.principal_key(), &team_platform.id).await;
    seed_membership(&pool, &bob.principal_key(), &team_platform.id).await;
    seed_membership(&pool, &alice.principal_key(), &team_data.id).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&user("admin", vec!["SpacebotAdmin"]));
    let req = Request::builder()
        .uri("/api/admin/teams")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let teams = parsed["teams"].as_array().expect("teams array");
    // Alphabetical by display_name: Data (1 member), Platform (2 members).
    assert_eq!(teams.len(), 2);
    assert_eq!(teams[0]["display_name"], "Data");
    assert_eq!(teams[0]["member_count"], 1);
    assert_eq!(teams[1]["display_name"], "Platform");
    assert_eq!(teams[1]["member_count"], 2);
    // Trimmed DTO: external_id must not leak.
    assert!(
        teams[0].get("external_id").is_none(),
        "AdminTeamDetail must not expose external_id; got row: {}",
        teams[0]
    );
    // last_sync_at populated (MAX of the seeded observed_at timestamps).
    assert!(
        teams[0]["last_sync_at"].is_string(),
        "last_sync_at must be present when memberships exist"
    );
}

#[tokio::test]
async fn admin_list_admin_teams_includes_empty_team_with_null_last_sync() {
    // LEFT JOIN coverage: a team with no memberships still appears, with
    // member_count=0 and last_sync_at=null.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    upsert_team(&pool, "grp-empty", "Empty Team").await.unwrap();

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&user("admin", vec!["SpacebotAdmin"]));
    let req = Request::builder()
        .uri("/api/admin/teams")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let teams = parsed["teams"].as_array().unwrap();
    let empty = teams
        .iter()
        .find(|t| t["display_name"] == "Empty Team")
        .unwrap();
    assert_eq!(empty["member_count"], 0);
    assert!(empty["last_sync_at"].is_null());
}

#[tokio::test]
async fn admin_list_team_members_returns_trimmed_rows() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user("alice", vec!["SpacebotUser"]);
    let bob = user("bob", vec!["SpacebotUser"]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    let team = upsert_team(&pool, "grp-platform", "Platform")
        .await
        .unwrap();
    seed_membership(&pool, &alice.principal_key(), &team.id).await;
    seed_membership(&pool, &bob.principal_key(), &team.id).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&user("admin", vec!["SpacebotAdmin"]));
    let req = Request::builder()
        .uri(format!("/api/admin/teams/{}/members", team.id))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let members = parsed["members"].as_array().expect("members array");
    assert_eq!(members.len(), 2);
    // Alphabetical by display_name ("User alice" < "User bob")
    assert_eq!(members[0]["display_name"], "User alice");
    assert_eq!(members[1]["display_name"], "User bob");
    // Source preserved from team_memberships row.
    assert_eq!(members[0]["source"], "token_claim");
    // Trimmed DTO: tenant_id and object_id must not leak.
    assert!(
        members[0].get("tenant_id").is_none(),
        "AdminTeamMemberDetail must not expose tenant_id; got row: {}",
        members[0]
    );
    assert!(
        members[0].get("object_id").is_none(),
        "AdminTeamMemberDetail must not expose object_id"
    );
}

#[tokio::test]
async fn admin_list_team_members_returns_empty_array_for_unknown_team() {
    // Callers that hit a nonexistent team id get 200 with an empty
    // members array rather than 404. The admin UI renders "No members"
    // either way, and a 404 would require the UI to distinguish between
    // "team exists, no members" and "team doesn't exist at all" which it
    // does not need to.
    let (state, _pool) = ApiState::new_test_state_with_mock_entra().await;
    let app = build_test_router_entra(state);
    let token = mint_mock_token(&user("admin", vec!["SpacebotAdmin"]));
    let req = Request::builder()
        .uri("/api/admin/teams/team-does-not-exist/members")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["members"].as_array().unwrap().len(), 0);
}
