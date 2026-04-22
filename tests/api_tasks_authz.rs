//! Phase 4 PR 2 T4.8 — handler-level authz tests for `src/api/tasks.rs`.
//!
//! Mirrors `tests/api_memories_authz.rs`: exercises the full stack
//! (Entra middleware with MockValidator, handler `check_read_with_audit`
//! / `check_write`, policy module against a real `resource_ownership`
//! row). Eight task handlers share a single ~45-line inline gate; a
//! non-owner read (get) + non-owner write (update) + admin bypass +
//! owner-200 + create-ownership + pool-None skip are enough to prove
//! the gate is wired without re-covering the policy module, which has
//! its own 15 tests in `tests/policy_table.rs`.
//!
//! Task URLs key on `task_number` (i64) but ownership rows key on the
//! UUID `task.id` (A-09). The handler fetches the task by number first,
//! then gates on its UUID; these tests create tasks via the handler so
//! the create path registers ownership on the right resource id.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use spacebot::api::ApiState;
use spacebot::api::test_support::build_test_router_entra;
use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::principals::Visibility;
use spacebot::auth::repository::{get_ownership, set_ownership, upsert_user_from_auth};
use spacebot::auth::roles::{ROLE_ADMIN, ROLE_USER};
use spacebot::auth::testing::mint_mock_token;
use spacebot::tasks::TaskStore;
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

/// Install a task store on the test state using the same pool the
/// authz middleware reads. Required because `ApiState::new_test_state_*`
/// leaves `task_store` unset (service returns 503 without it).
fn attach_task_store(state: &ApiState, pool: &sqlx::SqlitePool) {
    state.set_task_store(Arc::new(TaskStore::new(pool.clone())));
}

fn req_get_task(number: i64, bearer: &str) -> Request<Body> {
    Request::builder()
        .uri(format!("/api/tasks/{number}"))
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap()
}

fn req_update_task(number: i64, bearer: &str, body_json: &str) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(format!("/api/tasks/{number}"))
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body_json.to_string()))
        .unwrap()
}

fn req_create_task(bearer: &str, owner_agent_id: &str) -> Request<Body> {
    let body = serde_json::json!({
        "owner_agent_id": owner_agent_id,
        "title": "A task",
    })
    .to_string();
    Request::builder()
        .method("POST")
        .uri("/api/tasks")
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap()
}

/// Create a task directly via the store (bypassing the handler) and
/// register ownership against `owner`. Returns `(task_number, task_id)`.
async fn seed_task(pool: &sqlx::SqlitePool, owner: &AuthContext) -> (i64, String) {
    let store = TaskStore::new(pool.clone());
    let task = store
        .create(spacebot::tasks::CreateTaskInput {
            owner_agent_id: "agent-a".to_string(),
            assigned_agent_id: "agent-a".to_string(),
            title: "seeded".to_string(),
            description: None,
            status: spacebot::tasks::TaskStatus::PendingApproval,
            priority: spacebot::tasks::TaskPriority::Medium,
            subtasks: vec![],
            metadata: serde_json::json!({}),
            source_memory_id: None,
            created_by: "test".to_string(),
        })
        .await
        .unwrap();
    set_ownership(
        pool,
        "task",
        &task.id,
        None,
        &owner.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();
    (task.task_number, task.id)
}

#[tokio::test]
async fn non_owner_get_task_returns_404() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_task_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    let (number, _id) = seed_task(&pool, &alice).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let res = app.oneshot(req_get_task(number, &token)).await.unwrap();

    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "non-owner on alice's personal task must see 404 (hide existence), not 403"
    );
}

#[tokio::test]
async fn owner_get_task_returns_200() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_task_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    let (number, _id) = seed_task(&pool, &alice).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let res = app.oneshot(req_get_task(number, &token)).await.unwrap();

    assert_eq!(
        res.status(),
        StatusCode::OK,
        "owner must see 200 on their own task (got {:?})",
        res.status()
    );
}

#[tokio::test]
async fn admin_get_task_returns_200() {
    // Admin bypass: a SpacebotAdmin role skips per-resource ownership.
    // Regression guard against `is_admin` returning false on the tasks
    // handler gate.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_task_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let admin = user_ctx("admin-carol", vec![ROLE_ADMIN]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &admin).await.unwrap();
    let (number, _id) = seed_task(&pool, &alice).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&admin);
    let res = app.oneshot(req_get_task(number, &token)).await.unwrap();

    assert_eq!(
        res.status(),
        StatusCode::OK,
        "admin must bypass per-resource ownership on GET /tasks/{{number}}"
    );
}

#[tokio::test]
async fn non_owner_update_task_returns_404() {
    // check_write returns DenyReason::NotYours for a wrong owner, which
    // `to_status` maps to 404 (same hide-existence policy as read). Proves
    // the write gate fires on update_task; sibling write handlers
    // (delete/approve/execute/assign) share the same `check_write` block.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_task_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    let (number, _id) = seed_task(&pool, &alice).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let body = r#"{"title":"hijacked"}"#;
    let res = app
        .oneshot(req_update_task(number, &token, body))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "non-owner PUT on personal task must see 404 per DenyReason::NotYours mapping"
    );
}

#[tokio::test]
async fn create_task_assigns_ownership() {
    // A-12: the POST /tasks handler MUST `.await` set_ownership before
    // returning. A tokio::spawn fire-and-forget would leave a window
    // where the creator's immediate GET /tasks/{number} races into a
    // NotOwned 404. The proof is an ownership row present synchronously
    // after the POST completes.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_task_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let res = app
        .oneshot(req_create_task(&token, "agent-a"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "create_task must succeed");

    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let task_id = parsed["task"]["id"].as_str().expect("task.id").to_string();

    let own = get_ownership(&pool, "task", &task_id)
        .await
        .unwrap()
        .expect("ownership row must be present synchronously after POST");
    assert_eq!(
        own.owner_principal_key,
        alice.principal_key(),
        "owner principal_key must be the creator (alice)"
    );
    assert_eq!(own.visibility, "personal");
}

fn req_delete_task(number: i64, bearer: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(format!("/api/tasks/{number}"))
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap()
}

fn req_list_tasks_by_owner(owner_agent_id: &str, bearer: &str) -> Request<Body> {
    Request::builder()
        .uri(format!("/api/tasks?owner_agent_id={owner_agent_id}"))
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn non_owner_delete_task_returns_404() {
    // Regression guard for the delete write-gate. Shares the inline
    // check_write block with update/approve/execute/assign; if a future
    // refactor drops the gate on delete specifically, this test fires.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_task_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    let (number, task_id) = seed_task(&pool, &alice).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let res = app
        .oneshot(req_delete_task(number, &token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "non-owner delete on alice's task must see 404, not 403 or 204"
    );
    // Ownership row must still exist — denied delete must not have
    // touched the row.
    let row = get_ownership(&pool, "task", &task_id).await.unwrap();
    assert!(
        row.is_some(),
        "ownership row must remain after denied delete"
    );
}

#[tokio::test]
async fn non_owner_list_tasks_by_owner_returns_404() {
    // Regression guard for the list_tasks info-disclosure surface:
    // `?owner_agent_id=<alice-agent>` from Bob must NOT return Alice's
    // task list. Before this gate landed, a caller could enumerate
    // another user's tasks by passing owner_agent_id without triggering
    // the agent_id gate. The fix gates on the first agent-scoped filter
    // present (agent_id -> owner_agent_id -> assigned_agent_id).
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_task_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    // Seed Alice as the owner of agent-a. Bob will attempt to list by
    // owner_agent_id=agent-a.
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
        .oneshot(req_list_tasks_by_owner("agent-a", &token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "non-owner listing by owner_agent_id must see 404 (hide agent existence), \
         not a task list"
    );
}

#[tokio::test]
async fn pool_none_skip_get_task() {
    // Regression guard for the early-startup / static-token fallback path.
    // When instance_pool is not attached, the get_task gate skips and the
    // handler proceeds to the store lookup (503 because task_store is
    // also unset without an attached pool). Assertion: NOT 401/403,
    // proving the request passed auth + the no-op authz skip.
    let state = ApiState::new_test_state_with_mock_entra_no_pool();
    let bob = user_ctx("bob", vec![ROLE_USER]);

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let res = app.oneshot(req_get_task(1, &token)).await.unwrap();

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
