//! Phase 5 PR #106 remediation (C4 + G2 — handler wire integration test).
//!
//! Task 5.7 replaced 25 Phase-4-era `tracing::info!("admin_read override...")`
//! stubs across 10 handler files with real `fire_admin_read_audit` calls,
//! and every `Access::Denied` branch now calls `fire_denied_audit`. This
//! file verifies that the handler → appender WIRE actually fires: an
//! admin-override read persists an `admin_read` row; a denied read persists
//! an `authz_denied` row.
//!
//! Uses `memories` as the proof-of-pattern handler family (the one Task 5.7
//! sweep was originally piloted against). Other 9 handler families follow
//! the same `fire_admin_read_audit` / `fire_denied_audit` shape — the
//! authz-gate-conformance review at PR #106 confirmed byte-uniformity
//! across all 63 gate sites.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use spacebot::api::ApiState;
use spacebot::api::test_support::build_test_router_entra;
use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::principals::Visibility;
use spacebot::auth::repository::{set_ownership, upsert_user_from_auth};
use spacebot::auth::roles::{ROLE_ADMIN, ROLE_USER};
use spacebot::auth::testing::mint_mock_token;
use std::sync::Arc;
use tower::ServiceExt as _;

fn user_ctx(oid: &str, roles: Vec<&str>) -> AuthContext {
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

fn req_list_memories(agent_id: &str, token: &str) -> Request<Body> {
    Request::builder()
        .uri(format!(
            "/api/agents/memories?agent_id={agent_id}&limit=10"
        ))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

async fn yield_for_spawned_append() {
    tokio::task::yield_now().await;
    tokio::task::yield_now().await;
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
}

#[tokio::test]
async fn admin_break_glass_read_persists_admin_read_row() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice-owner", vec![ROLE_USER]);
    let admin = user_ctx("admin-break-glass", vec![ROLE_ADMIN]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &admin).await.unwrap();
    set_ownership(
        &pool,
        "agent",
        "agent-alice-break-glass",
        None,
        &alice.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&admin);
    let _ = app
        .oneshot(req_list_memories("agent-alice-break-glass", &token))
        .await
        .unwrap();

    yield_for_spawned_append().await;

    let rows: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT action, principal_key, resource_type, resource_id \
         FROM audit_events WHERE action = 'admin_read'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        !rows.is_empty(),
        "expected an admin_read row after admin bypass; table had no admin_read event — handler emission may have regressed"
    );
    assert_eq!(rows[0].0, "admin_read");
    assert!(
        rows[0].1.contains("admin-break-glass"),
        "admin_read row should attribute to the admin principal; got {:?}",
        rows[0].1
    );
    assert_eq!(
        rows[0].2.as_deref(),
        Some("agent"),
        "admin_read resource_type should be 'agent' for the memories-on-agent break-glass scenario"
    );
    assert_eq!(
        rows[0].3.as_deref(),
        Some("agent-alice-break-glass"),
        "admin_read resource_id should carry the target agent id"
    );
}

#[tokio::test]
async fn non_owner_denied_read_persists_authz_denied_row() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice-owner-2", vec![ROLE_USER]);
    let bob = user_ctx("bob-denied", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    set_ownership(
        &pool,
        "agent",
        "agent-alice-denied-probe",
        None,
        &alice.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let res = app
        .oneshot(req_list_memories("agent-alice-denied-probe", &token))
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "non-owner read should return 404 (hide-existence policy)"
    );

    yield_for_spawned_append().await;

    type DeniedRow = (String, String, Option<String>, Option<String>, String);
    let rows: Vec<DeniedRow> = sqlx::query_as(
        "SELECT action, principal_key, resource_type, resource_id, result \
         FROM audit_events WHERE action = 'authz_denied'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        !rows.is_empty(),
        "expected an authz_denied row after non-owner read; table had no authz_denied event — handler emission may have regressed"
    );
    assert_eq!(rows[0].0, "authz_denied");
    assert!(
        rows[0].1.contains("bob-denied"),
        "authz_denied row should attribute to the denied principal; got {:?}",
        rows[0].1
    );
    assert_eq!(rows[0].4, "denied");
    assert_eq!(
        rows[0].2.as_deref(),
        Some("agent"),
        "authz_denied resource_type should be 'agent' at the memories handler"
    );
    assert_eq!(
        rows[0].3.as_deref(),
        Some("agent-alice-denied-probe"),
        "authz_denied resource_id should carry the target agent id"
    );
}
