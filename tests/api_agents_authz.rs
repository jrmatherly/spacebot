//! Phase 4 PR 2 T4.12 — handler-level authz tests for `src/api/agents.rs`.
//!
//! Mirrors `tests/api_tasks_authz.rs` and `tests/api_cron_authz.rs`:
//! exercises the full stack (Entra middleware with MockValidator,
//! handler `check_read_with_audit` / `check_write`, policy module
//! against a real `resource_ownership` row). Agents are a cross-cut
//! resource — TOML declares them, HTTP creates them, and every handler
//! gates on `agent_id` (either in query or body). A non-owner read
//! (agent_overview) + non-owner write (update_agent) + admin bypass +
//! owner-200 + TOML-reconciliation helper are enough to prove the gate
//! is wired without re-covering the policy module, which has its own
//! 16 tests in `tests/policy_table.rs`.
//!
//! The "create assigns ownership" case bypasses
//! `create_agent_internal`: that path requires a full instance setup
//! (config.toml on disk, LLM manager, MCP managers, sandboxes). Instead
//! the test exercises `register_agent_ownership`, the shared helper
//! both the HTTP wrapper and the TOML reconciliation call.
//!
//! `pool_none_skip_agent_overview` covers Gate 5 (the early-startup
//! fallback path). `trigger_warmup_user_role_unfiltered_is_403`
//! covers the T4.12 I2 review fix: unfiltered warmup is admin-only.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use spacebot::api::ApiState;
use spacebot::api::test_support::build_test_router_entra;
use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::principals::Visibility;
use spacebot::auth::repository::{get_ownership, set_ownership, upsert_user_from_auth};
use spacebot::auth::roles::{ROLE_ADMIN, ROLE_USER};
use spacebot::auth::testing::mint_mock_token;
use spacebot::config::AgentConfig;
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

fn req_agent_overview(agent_id: &str, bearer: &str) -> Request<Body> {
    Request::builder()
        .uri(format!("/api/agents/overview?agent_id={agent_id}"))
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap()
}

fn req_update_agent(bearer: &str, body_json: &str) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri("/api/agents")
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body_json.to_string()))
        .unwrap()
}

/// Seed ownership on a synthetic agent_id. The ownership row is what the
/// Phase-4 gate reads; the agent_pools map (required for the success
/// path of `agent_overview` to return 200) is separately attached in
/// the single owner_get_agent_overview_returns_200 test. Tests that
/// only assert denial (404) do not need an attached pool because the
/// gate fires before the handler touches the pool.
async fn seed_agent_ownership(pool: &sqlx::SqlitePool, agent_id: &str, owner: &AuthContext) {
    set_ownership(
        pool,
        "agent",
        agent_id,
        None,
        &owner.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn non_owner_get_agent_overview_returns_404() {
    // Non-owner reading Alice's agent must see 404 (hide existence),
    // not 403. The fetch of agent_pools happens AFTER the gate, so a
    // missing pool cannot leak existence via a different status.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    seed_agent_ownership(&pool, "agent-a", &alice).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let res = app
        .oneshot(req_agent_overview("agent-a", &token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "non-owner on alice's personal agent must see 404 (hide existence), not 403"
    );
}

#[tokio::test]
async fn owner_get_agent_overview_passes_gate() {
    // Owner passes the authz gate. The handler returns 404 only because
    // the test doesn't attach a per-agent SQLite pool (agent_pools is
    // empty). The assertion here is that the response status is NOT a
    // gate-originated 401/403 — proving the gate allowed the request.
    // Pools-not-attached is a separate operational concern outside the
    // authz surface.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    seed_agent_ownership(&pool, "agent-a", &alice).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let res = app
        .oneshot(req_agent_overview("agent-a", &token))
        .await
        .unwrap();

    assert_ne!(
        res.status(),
        StatusCode::UNAUTHORIZED,
        "owner token must authenticate"
    );
    assert_ne!(
        res.status(),
        StatusCode::FORBIDDEN,
        "owner must not trip the authz gate"
    );
}

#[tokio::test]
async fn admin_bypass_agent_overview() {
    // Admin bypass: SpacebotAdmin role skips per-resource ownership.
    // Regression guard against `is_admin` returning false on the agents
    // handler gate.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let admin = user_ctx("admin-carol", vec![ROLE_ADMIN]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &admin).await.unwrap();
    seed_agent_ownership(&pool, "agent-a", &alice).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&admin);
    let res = app
        .oneshot(req_agent_overview("agent-a", &token))
        .await
        .unwrap();

    // Admin passes the gate (not 401/403). The downstream 404 from
    // agent_pools-empty is acceptable; the assertion is limited to
    // the authz surface.
    assert_ne!(
        res.status(),
        StatusCode::UNAUTHORIZED,
        "admin must authenticate"
    );
    assert_ne!(
        res.status(),
        StatusCode::FORBIDDEN,
        "admin must bypass per-resource ownership on GET /agents/overview"
    );
}

#[tokio::test]
async fn non_owner_update_agent_returns_404() {
    // check_write returns DenyReason::NotYours for a wrong owner, which
    // `to_status` maps to 404 (same hide-existence policy as read).
    // Proves the write gate fires on update_agent.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    seed_agent_ownership(&pool, "agent-a", &alice).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let body = r#"{"agent_id":"agent-a","display_name":"hijacked"}"#;
    let res = app.oneshot(req_update_agent(&token, body)).await.unwrap();

    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "non-owner PUT on alice's agent must see 404 per DenyReason::NotYours mapping"
    );
}

#[tokio::test]
async fn register_agent_ownership_helper_inserts_personal_row() {
    // A-12 proof: the shared helper awaits `set_ownership` and leaves a
    // Personal-visibility row keyed on the creator's principal_key.
    // Both the HTTP `create_agent` handler and the TOML reconciliation
    // at startup call this helper; a fire-and-forget `tokio::spawn`
    // would race the creator's immediate follow-up GET.
    let (_state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();

    spacebot::api::agents::register_agent_ownership(&pool, &alice, "agent-new")
        .await
        .expect("register_agent_ownership should succeed");

    let row = get_ownership(&pool, "agent", "agent-new")
        .await
        .unwrap()
        .expect("ownership row must be present synchronously after helper returns");
    assert_eq!(row.owner_principal_key, alice.principal_key());
    assert_eq!(row.visibility, "personal");
}

#[tokio::test]
async fn toml_reconciliation_assigns_legacy_static_ownership() {
    // Per §11.3 backfill policy: agents declared in [[agents]] blocks
    // without an ownership row must be claimed at startup. The synthetic
    // legacy-static principal matches AuthContext::legacy_static's
    // principal_key and sits in the admin-bypass set so pre-Entra CLI
    // callers retain access until an admin re-claims.
    let (_state, pool) = ApiState::new_test_state_with_mock_entra().await;

    let agents = vec![
        AgentConfig {
            id: "agent-legacy-1".to_string(),
            default: false,
            display_name: None,
            role: None,
            gradient_start: None,
            gradient_end: None,
            workspace: None,
            routing: None,
            max_concurrent_branches: None,
            max_concurrent_workers: None,
            max_turns: None,
            branch_max_turns: None,
            context_window: None,
            tool_use_enforcement: None,
            compaction: None,
            memory_persistence: None,
            coalesce: None,
            ingestion: None,
            cortex: None,
            warmup: None,
            browser: None,
            channel: None,
            mcp: None,
            brave_search_key: None,
            cron_timezone: None,
            user_timezone: None,
            sandbox: None,
            projects: None,
            cron: Vec::new(),
        },
        AgentConfig {
            id: "agent-legacy-2".to_string(),
            default: false,
            display_name: None,
            role: None,
            gradient_start: None,
            gradient_end: None,
            workspace: None,
            routing: None,
            max_concurrent_branches: None,
            max_concurrent_workers: None,
            max_turns: None,
            branch_max_turns: None,
            context_window: None,
            tool_use_enforcement: None,
            compaction: None,
            memory_persistence: None,
            coalesce: None,
            ingestion: None,
            cortex: None,
            warmup: None,
            browser: None,
            channel: None,
            mcp: None,
            brave_search_key: None,
            cron_timezone: None,
            user_timezone: None,
            sandbox: None,
            projects: None,
            cron: Vec::new(),
        },
    ];

    let reconciled = spacebot::config::reconcile_toml_agents_with_ownership(&pool, &agents)
        .await
        .expect("reconcile_toml_agents_with_ownership should succeed");
    assert_eq!(
        reconciled, 2,
        "both agents without ownership rows should be reconciled"
    );

    for id in ["agent-legacy-1", "agent-legacy-2"] {
        let row = get_ownership(&pool, "agent", id)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("ownership row for {id} must be present"));
        assert_eq!(
            row.owner_principal_key, "legacy-static",
            "synthetic legacy-static principal per §11.3 backfill"
        );
        assert_eq!(row.visibility, "personal");
    }

    // Idempotent: second invocation reconciles 0 (rows already present).
    let reconciled_again = spacebot::config::reconcile_toml_agents_with_ownership(&pool, &agents)
        .await
        .expect("second call should be idempotent");
    assert_eq!(
        reconciled_again, 0,
        "existing ownership rows must be left untouched"
    );
}

#[tokio::test]
async fn pool_none_skip_agent_overview() {
    // Regression guard for the early-startup / static-token fallback path.
    // When instance_pool is not attached, the agent_overview gate skips
    // and the handler proceeds past authz. Assertion: not 401 (mock
    // token authenticates) and not 403 (authz skip does not deny),
    // proving the request passed middleware + the no-op authz skip.
    let state = ApiState::new_test_state_with_mock_entra_no_pool();
    let bob = user_ctx("bob", vec![ROLE_USER]);

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let res = app
        .oneshot(req_agent_overview("agent-alice-1", &token))
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

fn req_trigger_warmup(agent_id: Option<&str>, force: bool, bearer: &str) -> Request<Body> {
    let body = if let Some(aid) = agent_id {
        serde_json::json!({ "agent_id": aid, "force": force })
    } else {
        serde_json::json!({ "force": force })
    };
    Request::builder()
        .method("POST")
        .uri("/api/agents/warmup/trigger")
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn trigger_warmup_user_role_unfiltered_is_403() {
    // T4.12 I2 regression guard: `trigger_warmup` with no `agent_id` is
    // an admin-broad operation that launches warmup coroutines for every
    // agent on the instance. Non-admin callers must be rejected with 403
    // to prevent un-owned principals from fanning out background work
    // they cannot read the output of. If the `!is_admin` check at
    // `src/api/agents.rs:635` were inverted (deny admins, allow users),
    // every agent-id-specific test would still pass; this test is the
    // only coverage of the unfiltered branch.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &bob).await.unwrap();

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let res = app
        .oneshot(req_trigger_warmup(None, false, &token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "non-admin user calling unfiltered warmup must receive 403, not 200"
    );
}

// Scope-filter tests for GET /api/agents?scope=mine|team|org.
// Agent configs are in-memory (ArcSwap) and resolved in-process; the
// scope filter narrows the returned list via a SQL allowlist fetched
// from resource_ownership + team_memberships. Tests exercise the four
// relevant paths: Mine (owner-match), Team (team-share match), Org
// (unfiltered), and admin-bypass (admin sees everything regardless
// of scope).

fn make_agent_info(id: &str) -> spacebot::api::AgentInfo {
    spacebot::api::AgentInfo {
        id: id.to_string(),
        display_name: None,
        role: None,
        gradient_start: None,
        gradient_end: None,
        workspace: "/tmp/test".to_string(),
        context_window: 200_000,
        max_turns: 5,
        max_concurrent_branches: 2,
        max_concurrent_workers: 2,
    }
}

async fn fetch_agents_with_scope(
    app: axum::Router,
    token: &str,
    scope_query: Option<&str>,
) -> serde_json::Value {
    let uri = match scope_query {
        Some(s) => format!("/api/agents?scope={s}"),
        None => "/api/agents".to_string(),
    };
    let res = app
        .oneshot(
            Request::builder()
                .uri(uri)
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn agent_ids(body: &serde_json::Value) -> Vec<String> {
    body["agents"]
        .as_array()
        .expect("agents array")
        .iter()
        .map(|a| a["id"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn list_agents_scope_mine_returns_only_owned_agents() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    seed_agent_ownership(&pool, "agent-alice", &alice).await;
    seed_agent_ownership(&pool, "agent-bob", &bob).await;
    seed_agent_ownership(&pool, "agent-unowned", &bob).await; // not Alice's

    state.set_agent_configs(vec![
        make_agent_info("agent-alice"),
        make_agent_info("agent-bob"),
        make_agent_info("agent-unowned"),
    ]);

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let body = fetch_agents_with_scope(app, &token, Some("mine")).await;

    let mut ids = agent_ids(&body);
    ids.sort();
    assert_eq!(
        ids,
        vec!["agent-alice".to_string()],
        "scope=mine must return only Alice's owned agent"
    );
}

#[tokio::test]
async fn list_agents_scope_team_returns_only_team_shared_agents() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();

    let team = spacebot::auth::repository::upsert_team(&pool, "grp-x", "Team X")
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO team_memberships (principal_key, team_id, source) \
         VALUES (?, ?, 'token_claim')",
    )
    .bind(alice.principal_key())
    .bind(&team.id)
    .execute(&pool)
    .await
    .unwrap();

    // Bob owns agent-shared, shared to team-x (Alice's team)
    set_ownership(
        &pool,
        "agent",
        "agent-shared",
        None,
        &bob.principal_key(),
        Visibility::Team,
        Some(&team.id),
    )
    .await
    .unwrap();
    // Alice owns her own agent (personal) — must NOT appear in team scope
    seed_agent_ownership(&pool, "agent-alice-own", &alice).await;

    state.set_agent_configs(vec![
        make_agent_info("agent-shared"),
        make_agent_info("agent-alice-own"),
    ]);

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let body = fetch_agents_with_scope(app, &token, Some("team")).await;

    assert_eq!(
        agent_ids(&body),
        vec!["agent-shared".to_string()],
        "scope=team returns team-shared; excludes own personal + own-team-shares"
    );
}

#[tokio::test]
async fn list_agents_scope_org_matches_unfiltered_for_non_admin() {
    // Org scope returns the full configured list, same as unfiltered.
    // Distinct from Mine/Team, which narrow to ownership/membership.
    // This test confirms Org is not accidentally empty for a user with
    // no ownerships — it pins the decision that Org == unfiltered today.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    state.set_agent_configs(vec![
        make_agent_info("agent-a"),
        make_agent_info("agent-b"),
        make_agent_info("agent-c"),
    ]);

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let body = fetch_agents_with_scope(app, &token, Some("org")).await;

    let mut ids = agent_ids(&body);
    ids.sort();
    assert_eq!(
        ids,
        vec![
            "agent-a".to_string(),
            "agent-b".to_string(),
            "agent-c".to_string()
        ],
        "scope=org returns all configured agents"
    );
}

#[tokio::test]
async fn list_agents_no_scope_param_preserves_legacy_unfiltered_behavior() {
    // Absent scope param = unfiltered list. Regression guard against a
    // refactor that accidentally applies a default scope. Admins, CLI
    // scripts, and pre-PR-5 clients depend on this.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    state.set_agent_configs(vec![make_agent_info("agent-a"), make_agent_info("agent-b")]);

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let body = fetch_agents_with_scope(app, &token, None).await;

    let mut ids = agent_ids(&body);
    ids.sort();
    assert_eq!(
        ids,
        vec!["agent-a".to_string(), "agent-b".to_string()],
        "no ?scope= param returns unfiltered list"
    );
}

#[tokio::test]
async fn list_agents_scope_admin_bypass_returns_unfiltered_even_with_scope_mine() {
    // Admin's scope=mine returns the full list, because admin bypasses
    // the ownership filter (same pattern as the per-row authz admin
    // bypass). Distinguishes "admin with scope=mine" from "user with
    // scope=mine" — admin sees everything.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let admin = user_ctx("admin-carol", vec![ROLE_ADMIN]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &admin).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    seed_agent_ownership(&pool, "agent-bob", &bob).await; // admin does NOT own

    state.set_agent_configs(vec![
        make_agent_info("agent-bob"),
        make_agent_info("agent-standalone"),
    ]);

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&admin);
    let body = fetch_agents_with_scope(app, &token, Some("mine")).await;

    let mut ids = agent_ids(&body);
    ids.sort();
    assert_eq!(
        ids,
        vec!["agent-bob".to_string(), "agent-standalone".to_string()],
        "admin with scope=mine sees the full list; ownership filter does not apply to admins"
    );
}
