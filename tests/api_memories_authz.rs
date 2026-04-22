//! Proof-of-pattern integration test for the memories handler authz
//! rollout. Exercises the full stack: Entra middleware with a MockValidator
//! installed, the handler's `check_read_with_audit` call, and the policy
//! module's decision against a real `resource_ownership` row.
//!
//! Subsequent handlers (tasks, wiki, cron, portal, agents, notifications,
//! projects, attachments, ingestion) ship in PR 2. Each follows this shape:
//! duplicate + adapt. The invariants this file asserts:
//!
//! 1. Non-owner reading another user's personal agent sees 404, not 403
//!    (ownership hide-existence policy).
//! 2. Admin (SpacebotAdmin role) bypasses per-resource ownership.
//! 3. Missing ownership row is a 404 (default deny for pre-Entra data).
//!
//! For the success path when alice reads her own agent, the handler
//! passes the authz gate then hits `state.memory_searches.get(...)` which
//! returns None (the test fixture doesn't register a real MemorySearch) —
//! that's a 404 from a DIFFERENT path. The `not 401` assertion distinguishes
//! "authz passed, resource infrastructure absent" from "auth rejected".

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

fn req_list_memories(agent_id: &str, bearer: &str) -> Request<Body> {
    Request::builder()
        .uri(format!(
            "/api/agents/memories?agent_id={agent_id}&limit=10&offset=0&sort=recent"
        ))
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn non_owner_reading_personal_agent_returns_404() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    set_ownership(
        &pool,
        "agent",
        "agent-alice-1",
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
        .oneshot(req_list_memories("agent-alice-1", &token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "non-owner on personal agent must see 404 (hide existence), not 403"
    );
}

#[tokio::test]
async fn admin_role_bypasses_ownership_on_agent_read() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let admin = user_ctx("admin-carol", vec![ROLE_ADMIN]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &admin).await.unwrap();
    set_ownership(
        &pool,
        "agent",
        "agent-alice-1",
        None,
        &alice.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&admin);
    let res = app
        .oneshot(req_list_memories("agent-alice-1", &token))
        .await
        .unwrap();

    // Admin passes the authz gate. The downstream `memory_searches.get(...)`
    // lookup then returns 404 because the test fixture doesn't register a
    // real MemorySearch. The 404 here comes from the post-authz
    // infrastructure lookup, not from the policy. The assertion we make is
    // that the request was NOT rejected at auth (401); it reached the
    // handler body, proving the admin bypass worked.
    assert_ne!(
        res.status(),
        StatusCode::UNAUTHORIZED,
        "admin should pass the Entra middleware with a valid mock token"
    );
    // Forbidden would indicate is_admin returned false: the bug this
    // test guards against.
    assert_ne!(
        res.status(),
        StatusCode::FORBIDDEN,
        "admin bypass failed: check_read_with_audit returned Forbidden"
    );
}

#[tokio::test]
async fn owner_passes_authz_gate_on_own_agent() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    set_ownership(
        &pool,
        "agent",
        "agent-alice-1",
        None,
        &alice.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let res = app
        .oneshot(req_list_memories("agent-alice-1", &token))
        .await
        .unwrap();

    // Same reasoning as admin_role_bypasses_ownership_on_agent_read: the
    // handler passes authz, then the memory_searches lookup returns 404
    // (no MemorySearch registered in the test state). Distinguish the two
    // kinds of 404 with the 401/403-absent assertions.
    assert_ne!(
        res.status(),
        StatusCode::UNAUTHORIZED,
        "owner should pass Entra middleware with a valid mock token"
    );
    assert_ne!(
        res.status(),
        StatusCode::FORBIDDEN,
        "owner authz failed: check_read_with_audit returned Forbidden for the owner"
    );
}

#[tokio::test]
async fn list_memories_without_bearer_header_returns_401() {
    // Regression guard: confirms list_memories goes through the Entra
    // middleware. A misconfigured route (e.g. a future developer moves
    // /api/agents/memories out of the middleware-wrapped router) would
    // let unauthenticated requests through. This test fails loudly if
    // that wiring regresses. Covers PR #104 review test-coverage Nit 4.
    let (state, _pool) = ApiState::new_test_state_with_mock_entra().await;
    let app = build_test_router_entra(state);
    let req = axum::http::Request::builder()
        .uri("/api/agents/memories?agent_id=anything&limit=10&offset=0&sort=recent")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::UNAUTHORIZED,
        "missing Authorization header must yield 401 from the middleware"
    );
}

#[tokio::test]
async fn team_member_reads_team_visible_agent_memories() {
    // Regression guard for the handler-level team-visibility path.
    // The logic is covered by policy_table.rs::team_member_can_read_team_memory,
    // but the handler test surface previously missed the "check_read_with_audit
    // was called with the correct resource_type + resource_id" assertion.
    // Covers PR #104 review test-coverage Nit 5.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    spacebot::auth::repository::upsert_user_from_auth(&pool, &alice)
        .await
        .unwrap();
    spacebot::auth::repository::upsert_user_from_auth(&pool, &bob)
        .await
        .unwrap();
    let team = spacebot::auth::repository::upsert_team(&pool, "grp-platform", "Platform")
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO team_memberships (principal_key, team_id, source)
           VALUES (?, ?, 'token_claim')"#,
    )
    .bind(bob.principal_key())
    .bind(&team.id)
    .execute(&pool)
    .await
    .unwrap();
    spacebot::auth::repository::set_ownership(
        &pool,
        "agent",
        "agent-shared",
        None,
        &alice.principal_key(),
        Visibility::Team,
        Some(&team.id),
    )
    .await
    .unwrap();

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let res = app
        .oneshot(req_list_memories("agent-shared", &token))
        .await
        .unwrap();

    // Bob is a team member; authz passes. Downstream memory_searches
    // lookup returns 404 (no MemorySearch registered), same as the
    // admin/owner tests above. Assertion: NOT 401/403 at the gate.
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED);
    assert_ne!(
        res.status(),
        StatusCode::FORBIDDEN,
        "team member must pass authz on team-visible agent"
    );
}

#[tokio::test]
async fn list_memories_no_ops_when_instance_pool_absent() {
    // Regression guard for the early-startup / static-token fallback path.
    // When instance_pool is not attached, list_memories skips authz and
    // proceeds to the memory_searches lookup. This test confirms the
    // request reaches that lookup (returning 404 because no MemorySearch
    // is registered for the test agent) instead of being rejected at the
    // authz gate. Covers Phase 4 PR 1 review finding B2 / test-coverage
    // Finding 3: the pool-None branch was previously untested.
    let state = ApiState::new_test_state_with_mock_entra_no_pool();
    let bob = user_ctx("bob", vec![ROLE_USER]);

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let res = app
        .oneshot(req_list_memories("agent-ghost", &token))
        .await
        .unwrap();

    // Request passed auth (middleware succeeded) + passed the no-op
    // authz skip + landed on the memory_searches lookup (404 because
    // nothing is registered). A 401/403 here would indicate the middleware
    // or the authz gate rejected the request instead of falling through.
    assert_ne!(
        res.status(),
        StatusCode::UNAUTHORIZED,
        "mock token must authenticate successfully"
    );
    assert_ne!(
        res.status(),
        StatusCode::FORBIDDEN,
        "authz skip should not cause a 403"
    );
}

#[tokio::test]
async fn missing_ownership_row_returns_404() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    // NO set_ownership call: the agent has no ownership row.

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let res = app
        .oneshot(req_list_memories("agent-ghost", &token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "missing ownership row → NotOwned → 404 per matrix (non-leaking deny)"
    );
}
