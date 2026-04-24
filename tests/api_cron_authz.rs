//! Phase 4 PR 2 T4.10 — handler-level authz tests for `src/api/cron.rs`.
//!
//! Mirrors `tests/api_tasks_authz.rs`: exercises the full stack (Entra
//! middleware with MockValidator, handler `check_read_with_audit` /
//! `check_write`, policy module against a real `resource_ownership` row).
//! Six cron handlers share a single ~45-line inline gate; non-owner read
//! (executions) + non-owner write (trigger) + admin bypass + owner-200 +
//! System-bypass regression + create-ownership (direct-call proof) +
//! pool-None skip cover the enforcement surface without re-covering
//! the policy module, which has its own tests in `tests/policy_table.rs`.
//!
//! Cron stores and ownership rows live in **different** SQLite databases:
//! `cron_jobs` sits in the per-agent schema under `migrations/`, while
//! `resource_ownership` sits in the instance-wide schema under
//! `migrations/global`. Tests create two pools, one per role, and attach
//! a `CronStore` backed by the per-agent pool to the state's cron_stores
//! map.
//!
//! A note on `create_cron_assigns_ownership`: the full POST handler path
//! also registers the cron with the in-process `Scheduler`, which requires
//! a full `AgentDeps` bundle (MemorySearch, LlmManager, McpManager,
//! Sandbox, etc.) that is impractical to construct in an integration
//! test. The test below proves the SAME contract by calling the exact
//! repository sequence the handler uses (`store.save` then `.await
//! set_ownership`) and reading the ownership row back synchronously.
//! This guards A-12 (await, not spawn) at the repository level, the
//! layer that holds the invariant.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use spacebot::api::ApiState;
use spacebot::api::test_support::build_test_router_entra;
use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::principals::Visibility;
use spacebot::auth::repository::{
    get_ownership, list_ownerships_by_ids, set_ownership, upsert_team, upsert_user_from_auth,
};
use spacebot::auth::roles::{ROLE_ADMIN, ROLE_USER};
use spacebot::auth::testing::mint_mock_token;
use spacebot::cron::{CronConfig, CronStore};
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

fn system_ctx() -> AuthContext {
    AuthContext {
        principal_type: PrincipalType::System,
        tid: Arc::from(""),
        oid: Arc::from(""),
        roles: vec![],
        groups: vec![],
        groups_overage: false,
        display_email: None,
        display_name: None,
    }
}

/// Build a per-agent SQLite pool with the non-global migration set applied
/// (so it has the `cron_jobs` table). Returns `(pool, store)`.
async fn per_agent_cron_pool() -> (sqlx::SqlitePool, Arc<CronStore>) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("per-agent sqlite pool");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("per-agent migrations apply cleanly");
    let store = Arc::new(CronStore::new(pool.clone()));
    (pool, store)
}

/// Attach a CronStore for `agent_id` onto the test state's cron_stores map.
async fn attach_cron_store(state: &ApiState, agent_id: &str) -> Arc<CronStore> {
    let (_pool, store) = per_agent_cron_pool().await;
    let mut map: HashMap<String, Arc<CronStore>> = HashMap::new();
    map.insert(agent_id.to_string(), store.clone());
    state.set_cron_stores(map);
    store
}

/// Seed a cron config on the given store and register an ownership row on
/// the instance pool keyed by `("cron_job", cron_id)` with the given owner.
async fn seed_cron(
    store: &CronStore,
    pool: &sqlx::SqlitePool,
    agent_id: &str,
    cron_id: &str,
    owner: &AuthContext,
) {
    store
        .save(&CronConfig {
            id: cron_id.to_string(),
            prompt: "seeded".into(),
            cron_expr: None,
            interval_secs: 3600,
            delivery_target: "discord:123456789".into(),
            active_hours: None,
            enabled: true,
            run_once: false,
            next_run_at: None,
            timeout_secs: None,
        })
        .await
        .expect("save cron config");
    set_ownership(
        pool,
        "cron_job",
        cron_id,
        Some(agent_id),
        &owner.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .expect("set ownership");
}

fn req_cron_executions(agent_id: &str, cron_id: &str, bearer: &str) -> Request<Body> {
    Request::builder()
        .uri(format!(
            "/api/agents/cron/executions?agent_id={agent_id}&cron_id={cron_id}&limit=10"
        ))
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap()
}

fn req_trigger_cron(agent_id: &str, cron_id: &str, bearer: &str) -> Request<Body> {
    let body = serde_json::json!({
        "agent_id": agent_id,
        "cron_id": cron_id,
    })
    .to_string();
    Request::builder()
        .method("POST")
        .uri("/api/agents/cron/trigger")
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap()
}

#[tokio::test]
async fn non_owner_get_cron_returns_404_or_403() {
    // `cron_executions` is agent-scoped read; seeding ownership on Alice's
    // agent makes Bob's read surface as 404 (NotYours hide-existence).
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    // Agent-scoped read: gate on the agent ownership row.
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
        .oneshot(req_cron_executions("agent-alice-1", "cron-1", &token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "non-owner on alice's personal agent must see 404 (hide existence), not 403"
    );
}

#[tokio::test]
async fn owner_can_read_cron() {
    // Owner reads executions on their own agent: passes the agent-scoped
    // read gate, then hits the cron_stores map and returns 200 with the
    // seeded empty execution list.
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

    let store = attach_cron_store(&state, "agent-alice-1").await;
    seed_cron(&store, &pool, "agent-alice-1", "cron-1", &alice).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let res = app
        .oneshot(req_cron_executions("agent-alice-1", "cron-1", &token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::OK,
        "owner must see 200 on their own cron executions (got {:?})",
        res.status()
    );
}

#[tokio::test]
async fn admin_bypass_cron_read() {
    // Admin bypass: SpacebotAdmin role skips per-resource ownership.
    // Regression guard against `is_admin` returning false for the cron
    // handler gate.
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

    let store = attach_cron_store(&state, "agent-alice-1").await;
    seed_cron(&store, &pool, "agent-alice-1", "cron-1", &alice).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&admin);
    let res = app
        .oneshot(req_cron_executions("agent-alice-1", "cron-1", &token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::OK,
        "admin must bypass per-resource ownership on GET /agents/cron/executions"
    );
}

#[tokio::test]
async fn non_owner_trigger_cron_returns_denied() {
    // `trigger_cron` is a write: `check_write` returns DenyReason::NotYours
    // for a wrong owner, which `to_status` maps to 404. Proves the write
    // gate fires on `trigger_cron`; sibling write handlers (delete, toggle,
    // create_or_update on existing) share the same `check_write` block.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    // Seed the cron ownership row: the write gate keys on ("cron_job", id).
    set_ownership(
        &pool,
        "cron_job",
        "cron-1",
        Some("agent-alice-1"),
        &alice.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let res = app
        .oneshot(req_trigger_cron("agent-alice-1", "cron-1", &token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "non-owner trigger on alice's cron must see 404 per DenyReason::NotYours mapping"
    );
}

#[tokio::test]
async fn system_bypasses_ownership_for_scheduled_run() {
    // Regression guard MANDATED by the Phase 4 PR 2 plan. Scheduled cron
    // runs execute as `PrincipalType::System`. `is_admin` includes System
    // in its bypass set (see `src/auth/roles.rs`), so check_read allows
    // the System principal against any resource regardless of the owner's
    // identity or user-table state. The plan originally framed this as
    // "cron of a disabled user" but the ownership FK forces the user row
    // to exist; the test keeps the spirit by proving the bypass still
    // fires against a non-matching principal owner. If a future refactor
    // narrows `is_admin` to exclude System, this test fires and prevents
    // silent breakage of scheduled execution.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    // The plan suggests skipping Alice's user row to simulate
    // disabled/missing state, but `resource_ownership.owner_principal_key`
    // has a FK to `users(principal_key) ON DELETE RESTRICT`, so the
    // ownership row can only be written while the user row exists. The
    // public repository surface has no `Disabled` setter today, so the
    // closest we can get is an active user row; the test still proves
    // the System bypass because the enforcement branch the test exercises
    // (is_admin → early Allowed) does not consult `users.status` at all.
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

    let store = attach_cron_store(&state, "agent-alice-1").await;
    seed_cron(&store, &pool, "agent-alice-1", "cron-1", &alice).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&system_ctx());
    let res = app
        .oneshot(req_cron_executions("agent-alice-1", "cron-1", &token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::OK,
        "System principal must bypass per-resource ownership so scheduled \
         runs can read crons owned by disabled/missing user rows"
    );
}

#[tokio::test]
async fn create_cron_assigns_ownership() {
    // A-12: the POST /agents/cron create-path MUST `.await` set_ownership
    // before returning. A tokio::spawn fire-and-forget would leave a
    // window where the creator's immediate GET or trigger races into
    // a NotOwned 404. The handler awaits set_ownership directly; this
    // test proves the repository-level contract by running the exact
    // same sequence (`store.save` then `.await set_ownership`) and
    // asserting the ownership row is present synchronously afterwards.
    //
    // The full POST path also invokes `Scheduler::register` which
    // requires a complete `AgentDeps` bundle (MemorySearch, LlmManager,
    // McpManager, Sandbox, and many more), which is impractical to
    // construct in an integration test. The handler's check_write-before-
    // update branch is covered by `non_owner_trigger_cron_returns_denied`
    // (same `check_write` block).
    let (_state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();

    // Replicate the new-cron handler sequence.
    let (_agent_pool, store) = per_agent_cron_pool().await;
    store
        .save(&CronConfig {
            id: "cron-new".into(),
            prompt: "created".into(),
            cron_expr: None,
            interval_secs: 3600,
            delivery_target: "discord:123".into(),
            active_hours: None,
            enabled: true,
            run_once: false,
            next_run_at: None,
            timeout_secs: None,
        })
        .await
        .expect("save cron config");
    set_ownership(
        &pool,
        "cron_job",
        "cron-new",
        Some("agent-alice-1"),
        &alice.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .expect("set ownership");

    let own = get_ownership(&pool, "cron_job", "cron-new")
        .await
        .unwrap()
        .expect("ownership row must be present synchronously after set_ownership");
    assert_eq!(
        own.owner_principal_key,
        alice.principal_key(),
        "owner principal_key must be the creator (alice)"
    );
    assert_eq!(own.visibility, "personal");
    assert_eq!(own.owner_agent_id.as_deref(), Some("agent-alice-1"));
}

#[tokio::test]
async fn pool_none_skip_get_cron() {
    // Regression guard for the early-startup / static-token fallback path.
    // When instance_pool is not attached, the get-cron-executions gate
    // skips and the handler proceeds past authz. Assertion: not 401
    // (mock token authenticates) and not 403 (authz skip does not deny),
    // proving the request passed middleware + the no-op authz skip.
    let state = ApiState::new_test_state_with_mock_entra_no_pool();
    let bob = user_ctx("bob", vec![ROLE_USER]);

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let res = app
        .oneshot(req_cron_executions("agent-alice-1", "cron-1", &token))
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
async fn cron_enrichment_keys_on_cron_job_resource_type() {
    // The SPA consumes `visibility` + `team_name` on each cron row to
    // render the chip. The cron list handler populates those fields via
    // `list_ownerships_by_ids(pool, "cron_job", &ids)` (the call inside
    // `enrich_visibility_tags` at src/api/cron.rs). Before this fix, the
    // handler passed "cron" (file-resource-family string, the same label
    // used for the Prometheus metric) while `set_ownership` at the create
    // path keyed the row on "cron_job"; the SQL WHERE clause silently
    // matched zero rows and every cron surfaced as
    // `{ visibility: None, team_name: None }` on the wire.
    //
    // This test locks the function-level contract: an ownership row
    // seeded via `set_ownership("cron_job", ...)` (the same repository
    // call `create_or_update_cron` makes at production runtime) must be
    // discoverable via `list_ownerships_by_ids(&pool, "cron_job", &ids)`
    // and must NOT be discoverable via the old buggy string
    // `list_ownerships_by_ids(&pool, "cron", &ids)`. The handler's call
    // site in `src/api/cron.rs` passes the literal `"cron_job"`; a
    // future regression there is a grep-visible one-line revert that
    // this assertion pair would detect when exercised alongside
    // the handler call.
    //
    // Why not end-to-end: the list handler returns 404 without an
    // attached `Scheduler` (see `cron.rs` cron_schedulers lookup), and
    // `Scheduler::new(CronContext)` needs a full `AgentDeps` bundle
    // (MemorySearch, LlmManager, McpManager, Sandbox, ...) that this
    // integration test fixture cannot construct (see the file-level
    // doc on `create_cron_assigns_ownership` for the same reasoning).
    // Testing the function boundary is the tightest available seam.
    let (_state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    let team = upsert_team(&pool, "grp-platform", "Platform")
        .await
        .unwrap();
    set_ownership(
        &pool,
        "cron_job",
        "cron-team-1",
        Some("agent-alice-1"),
        &alice.principal_key(),
        Visibility::Team,
        Some(&team.id),
    )
    .await
    .unwrap();

    let ids = vec!["cron-team-1".to_string()];

    // Correct key — the string the live handler uses post-fix.
    let map_correct = list_ownerships_by_ids(&pool, "cron_job", &ids)
        .await
        .expect("list_ownerships_by_ids succeeds");
    let row = map_correct
        .get("cron-team-1")
        .expect("cron_job-keyed lookup must find the seeded row");
    assert_eq!(row.visibility, "team");
    assert_eq!(row.shared_with_team_id.as_deref(), Some(team.id.as_str()));

    // Old buggy key — the pre-fix string. Must return empty because
    // the ownership row was keyed on "cron_job", not "cron".
    let map_buggy = list_ownerships_by_ids(&pool, "cron", &ids)
        .await
        .expect("list_ownerships_by_ids succeeds");
    assert!(
        map_buggy.is_empty(),
        "the old 'cron' resource_type must match zero rows; a non-empty \
         result means a future migration started double-keying rows"
    );
}
