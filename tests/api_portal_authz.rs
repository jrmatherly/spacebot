//! Phase 4 PR 2 T4.11 — handler-level authz tests for `src/api/portal.rs`.
//!
//! Mirrors `tests/api_tasks_authz.rs` and `tests/api_memories_authz.rs`:
//! exercises the full stack (Entra middleware with MockValidator,
//! per-handler inline `check_read_with_audit` / `check_write` / pre-check
//! `get_ownership` + `set_ownership` create blocks, real
//! `resource_ownership` row against the same pool the middleware reads).
//!
//! The load-bearing test here is
//! `create_portal_conversation_assigns_personal_ownership`: portal
//! conversations are private user chats and the Phase 4 plan §12 A-2
//! names this as the single most identity-sensitive table. Any future
//! refactor that flips the default to `Visibility::Org` leaks one
//! user's chat history tenant-wide; this test is the regression guard
//! against that.
//!
//! Portal URLs use `session_id` (a caller-supplied, UUID-shaped string)
//! as the per-conversation resource id. The handler consults
//! `resource_type = "portal_conversation"` with that bare id (A-09).
//! `portal_send` has an auto-create path that `.await`s `set_ownership`
//! when the session is new — not covered directly here since create via
//! `POST /portal/conversations` exercises the same ownership write
//! surface and is simpler to assert.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use spacebot::api::ApiState;
use spacebot::api::test_support::build_test_router_entra;
use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::principals::Visibility;
use spacebot::auth::repository::{
    get_ownership, set_ownership, upsert_team, upsert_user_from_auth,
};
use spacebot::auth::roles::{ROLE_ADMIN, ROLE_USER};
use spacebot::auth::testing::mint_mock_token;
use sqlx::sqlite::SqlitePoolOptions;
use std::collections::HashMap;
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

/// Seed an in-memory per-agent pool with the portal_conversations schema
/// and wire it into `state.agent_pools` under `agent_id`. Required
/// because the portal handlers reach `state.agent_pools[agent_id]` via
/// `conversation_store()` before/after the authz gate, and
/// `new_test_state_with_mock_entra()` only sets up the instance_pool.
async fn attach_agent_pool(state: &ApiState, agent_id: &str) -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("sqlite in-memory agent pool");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("per-agent migrations apply cleanly");
    let mut pools = HashMap::new();
    pools.insert(agent_id.to_string(), pool.clone());
    state.set_agent_pools(pools);
    pool
}

fn req_portal_history(agent_id: &str, session_id: &str, bearer: &str) -> Request<Body> {
    Request::builder()
        .uri(format!(
            "/api/portal/history?agent_id={agent_id}&session_id={session_id}&limit=10"
        ))
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap()
}

fn req_update_portal(agent_id: &str, session_id: &str, bearer: &str) -> Request<Body> {
    let body = serde_json::json!({
        "agent_id": agent_id,
        "title": "hijacked",
    })
    .to_string();
    Request::builder()
        .method("PUT")
        .uri(format!("/api/portal/conversations/{session_id}"))
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap()
}

fn req_create_portal(agent_id: &str, bearer: &str) -> Request<Body> {
    let body = serde_json::json!({
        "agent_id": agent_id,
        "title": "My chat",
    })
    .to_string();
    Request::builder()
        .method("POST")
        .uri("/api/portal/conversations")
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap()
}

#[tokio::test]
async fn non_owner_portal_history_returns_404() {
    // Bob reading Alice's personal portal_conversation must see 404 (hide
    // existence), not 403. Guards the read gate on portal_history.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let _agent_pool = attach_agent_pool(&state, "agent-a").await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    let session_id = "portal-chat-session-alice-1";
    set_ownership(
        &pool,
        "portal_conversation",
        session_id,
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
        .oneshot(req_portal_history("agent-a", session_id, &token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "non-owner on alice's personal portal_conversation must see 404 (hide existence), not 403"
    );
}

#[tokio::test]
async fn owner_can_read_portal_history() {
    // Alice reading her own personal portal_conversation passes authz.
    // Downstream success depends on the per-agent pool being attached
    // (it is) — a 200 or any non-401/403 means the gate allowed through.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let _agent_pool = attach_agent_pool(&state, "agent-a").await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    let session_id = "portal-chat-session-alice-1";
    set_ownership(
        &pool,
        "portal_conversation",
        session_id,
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
        .oneshot(req_portal_history("agent-a", session_id, &token))
        .await
        .unwrap();

    assert_ne!(
        res.status(),
        StatusCode::UNAUTHORIZED,
        "owner must authenticate (401 would indicate middleware/token issue)"
    );
    assert_ne!(
        res.status(),
        StatusCode::FORBIDDEN,
        "owner authz failed: check_read_with_audit denied the owner"
    );
    assert_ne!(
        res.status(),
        StatusCode::NOT_FOUND,
        "owner must NOT see 404 on their own portal_conversation"
    );
}

#[tokio::test]
async fn admin_bypass_portal_read() {
    // Admin bypass: a SpacebotAdmin role skips per-resource ownership.
    // Regression guard against `is_admin` returning false on the portal
    // handler gate.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let _agent_pool = attach_agent_pool(&state, "agent-a").await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let admin = user_ctx("admin-carol", vec![ROLE_ADMIN]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &admin).await.unwrap();
    let session_id = "portal-chat-session-alice-1";
    set_ownership(
        &pool,
        "portal_conversation",
        session_id,
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
        .oneshot(req_portal_history("agent-a", session_id, &token))
        .await
        .unwrap();

    assert_ne!(
        res.status(),
        StatusCode::UNAUTHORIZED,
        "admin must authenticate with a valid mock token"
    );
    assert_ne!(
        res.status(),
        StatusCode::FORBIDDEN,
        "admin bypass failed: check_read_with_audit returned Forbidden"
    );
    assert_ne!(
        res.status(),
        StatusCode::NOT_FOUND,
        "admin must bypass personal-ownership 404 on portal_conversation"
    );
}

#[tokio::test]
async fn non_owner_update_portal_conversation_denied() {
    // check_write returns DenyReason::NotYours for a wrong owner, mapped
    // to 404 (same hide-existence policy as read). Proves the write gate
    // fires on update_portal_conversation; delete shares the same
    // check_write block and covers by extension.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let _agent_pool = attach_agent_pool(&state, "agent-a").await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    let session_id = "portal-chat-session-alice-1";
    set_ownership(
        &pool,
        "portal_conversation",
        session_id,
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
        .oneshot(req_update_portal("agent-a", session_id, &token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "non-owner PUT on personal portal_conversation must see 404 per NotYours mapping"
    );
    // Ownership row is untouched by the denied write.
    let row = get_ownership(&pool, "portal_conversation", session_id)
        .await
        .unwrap()
        .expect("ownership row survives denied update");
    assert_eq!(row.owner_principal_key, alice.principal_key());
}

#[tokio::test]
async fn create_portal_conversation_assigns_personal_ownership() {
    // LOAD-BEARING REGRESSION GUARD (§12 A-2):
    //
    // Portal conversations are private user chats — the single most
    // identity-sensitive table in the system. The default visibility on
    // create MUST be `Personal`. A convenience default to `Org` or `Team`
    // would leak every user's chat history to the rest of the tenant.
    //
    // This test asserts:
    //   1. POST /portal/conversations succeeds (200).
    //   2. An ownership row is present SYNCHRONOUSLY after the POST
    //      (A-12: `.await` set_ownership, never `tokio::spawn`).
    //   3. The row's `owner_principal_key` is the creator (alice).
    //   4. The row's `visibility` is EXACTLY "personal", not "org" or
    //      "team". Flipping this default is a tenant-wide data leak.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let _agent_pool = attach_agent_pool(&state, "agent-a").await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    // Alice owns agent-a so the agent-scoped check_write gate added to
    // create_portal_conversation lets her create conversations under it.
    set_ownership(
        &pool,
        "agent",
        "agent-a",
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
        .oneshot(req_create_portal("agent-a", &token))
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "create_portal_conversation must succeed"
    );

    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let conversation_id = parsed["conversation"]["id"]
        .as_str()
        .expect("conversation.id in response")
        .to_string();

    let own = get_ownership(&pool, "portal_conversation", &conversation_id)
        .await
        .unwrap()
        .expect("ownership row must be present synchronously after POST");
    assert_eq!(
        own.owner_principal_key,
        alice.principal_key(),
        "owner_principal_key must be the creator (alice)"
    );
    assert_eq!(
        own.visibility, "personal",
        "portal_conversation visibility MUST default to Personal (§12 A-2). \
         Any other value is a tenant-wide data leak."
    );
}

#[tokio::test]
async fn non_agent_owner_create_portal_conversation_returns_404() {
    // Regression guard for the T4.11 code-quality-review Critical finding:
    // before this gate landed, any authenticated caller could POST to
    // /portal/conversations with any `agent_id` and mint a private
    // conversation row under that agent (then self-register as owner).
    // The fix adds `check_write("agent", &request.agent_id)` before
    // store.create. This test proves a caller with no claim to the agent
    // sees 404 (hide existence) rather than the conversation being
    // created out from under the real owner.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let _agent_pool = attach_agent_pool(&state, "agent-a").await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    // Alice owns agent-a. Bob should not be able to create conversations
    // under it.
    set_ownership(
        &pool,
        "agent",
        "agent-a",
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
        .oneshot(req_create_portal("agent-a", &token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "non-agent-owner create on alice's agent must see 404, \
         not a new conversation row with bob as owner"
    );
    // Belt-and-suspenders: no conversation ownership rows should exist
    // for agent-a since the gate fired before store.create.
    let rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM resource_ownership WHERE resource_type = 'portal_conversation'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        rows, 0,
        "denied create must not leave a conversation ownership row"
    );
}

#[tokio::test]
async fn pool_none_skip_portal_history() {
    // Regression guard for the early-startup / static-token fallback path.
    // When instance_pool is not attached, the portal_history gate skips
    // and the handler proceeds to the agent_pools lookup (404 because no
    // per-agent pool is registered without a main pool). Assertion: NOT
    // 401/403, proving auth passed + the authz skip was a no-op.
    let state = ApiState::new_test_state_with_mock_entra_no_pool();
    let bob = user_ctx("bob", vec![ROLE_USER]);

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let res = app
        .oneshot(req_portal_history("agent-ghost", "session-ghost", &token))
        .await
        .unwrap();

    assert_ne!(
        res.status(),
        StatusCode::UNAUTHORIZED,
        "mock token must authenticate successfully"
    );
    assert_ne!(
        res.status(),
        StatusCode::FORBIDDEN,
        "authz skip must not cause a 403"
    );
}

#[tokio::test]
async fn list_portal_conversations_enriches_team_scoped_conversation_with_chip_fields() {
    // The SPA consumes `visibility` + `team_name` on each conversation
    // row to render the chip. This pins the list-endpoint wire shape so
    // a regression to `Vec<PortalConversationSummary>` (chip absent) or
    // a drift in the resource_type string used at enrichment trips CI
    // before the SPA notices. Resource_type is "portal_conversation" at
    // all three code paths (set_ownership, check_write, enrich).
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let agent_pool = attach_agent_pool(&state, "agent-a").await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    let team = upsert_team(&pool, "grp-platform", "Platform")
        .await
        .unwrap();
    // Alice owns agent-a so the agent-scoped read gate on list passes.
    set_ownership(
        &pool,
        "agent",
        "agent-a",
        None,
        &alice.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();

    // Seed a portal_conversation row directly via the store (same
    // insert path the handler uses at create time).
    let store = spacebot::conversation::PortalConversationStore::new(agent_pool);
    let convo = store
        .create("agent-a", Some("Team Chat"), None)
        .await
        .expect("create portal conversation");

    // Upsert the ownership row to Team scope. `set_ownership` is an
    // upsert on (resource_type, resource_id), so this overrides any
    // Personal default a future auto-create path might introduce.
    set_ownership(
        &pool,
        "portal_conversation",
        &convo.id,
        Some("agent-a"),
        &alice.principal_key(),
        Visibility::Team,
        Some(&team.id),
    )
    .await
    .unwrap();

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let req = Request::builder()
        .uri("/api/portal/conversations?agent_id=agent-a")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(res.into_body(), 64 * 1024)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let conversations = body["conversations"]
        .as_array()
        .expect("conversations array present");
    let row = conversations
        .iter()
        .find(|c| c["id"] == convo.id.as_str())
        .expect("seeded conversation present in list response");
    assert_eq!(
        row["visibility"].as_str(),
        Some("team"),
        "chip visibility field must be present on team-scoped conversation"
    );
    assert_eq!(
        row["team_name"].as_str(),
        Some("Platform"),
        "chip team_name must resolve to the team's display_name"
    );
    // Confirm the flattened summary fields still cross the wire
    // (additive shape via #[serde(flatten)], not a rewrap).
    assert_eq!(row["agent_id"].as_str(), Some("agent-a"));
    assert_eq!(row["title"].as_str(), Some("Team Chat"));
}
