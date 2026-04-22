//! Phase 4 PR 2 T4.13a — handler-level authz tests for
//! `src/api/notifications.rs`.
//!
//! Mirrors `tests/api_tasks_authz.rs` / `tests/api_wiki_authz.rs`:
//! exercises the full stack (Entra middleware with MockValidator,
//! handler `check_read_with_audit` / `check_write`, policy module
//! against a real `resource_ownership` row).
//!
//! Notifications have no user-facing POST endpoint — all creations
//! happen server-side via `ApiState::emit_notification`, so there is
//! no `create_notification_assigns_ownership` test in this file.
//! Instead we seed a notification via the store and register ownership
//! directly, then prove the per-id write gates (`mark_read`,
//! `dismiss_notification`) consult the ownership row. List gating is
//! tested via the `agent_id` filter path, which is the only gated read
//! surface per the Phase-5 TODO on unfiltered listings.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use spacebot::api::ApiState;
use spacebot::api::test_support::build_test_router_entra;
use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::principals::Visibility;
use spacebot::auth::repository::{set_ownership, upsert_user_from_auth};
use spacebot::auth::roles::{ROLE_ADMIN, ROLE_USER};
use spacebot::auth::testing::mint_mock_token;
use spacebot::notifications::{
    NewNotification, NotificationKind, NotificationSeverity, NotificationStore,
};
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

/// Install a notification store on the test state using the same pool
/// the authz middleware reads. Required because
/// `ApiState::new_test_state_*` leaves `notification_store` unset
/// (handlers return 503 without it).
fn attach_notification_store(state: &ApiState, pool: &sqlx::SqlitePool) {
    state.set_notification_store(Arc::new(NotificationStore::new(pool.clone())));
}

fn req_list_notifications_by_agent(agent_id: &str, bearer: &str) -> Request<Body> {
    Request::builder()
        .uri(format!("/api/notifications?agent_id={agent_id}"))
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap()
}

fn req_mark_read(id: &str, bearer: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(format!("/api/notifications/{id}/read"))
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap()
}

fn req_dismiss(id: &str, bearer: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(format!("/api/notifications/{id}/dismiss"))
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap()
}

/// Seed a notification row via the store and register ownership against
/// `owner`. Returns the new notification id.
async fn seed_notification(pool: &sqlx::SqlitePool, owner: &AuthContext) -> String {
    let store = NotificationStore::new(pool.clone());
    let notification = store
        .insert(NewNotification {
            kind: NotificationKind::CortexObservation,
            severity: NotificationSeverity::Info,
            title: "seeded".to_string(),
            body: None,
            agent_id: Some("agent-a".to_string()),
            related_entity_type: None,
            related_entity_id: None,
            action_url: None,
            metadata: None,
        })
        .await
        .unwrap()
        .expect("insert returned a row");
    set_ownership(
        pool,
        "notification",
        &notification.id,
        None,
        &owner.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();
    notification.id
}

#[tokio::test]
async fn non_owner_mark_read_notification_returns_404() {
    // check_write returns DenyReason::NotYours for a wrong owner, which
    // `to_status` maps to 404 (same hide-existence policy as reads).
    // Sibling write handler (dismiss_notification) shares the same
    // inline check_write block.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_notification_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    let id = seed_notification(&pool, &alice).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let res = app.oneshot(req_mark_read(&id, &token)).await.unwrap();

    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "non-owner POST /notifications/{{id}}/read on alice's personal \
         notification must see 404 (hide existence), not 403"
    );
}

#[tokio::test]
async fn owner_mark_read_notification_returns_204() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_notification_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    let id = seed_notification(&pool, &alice).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let res = app.oneshot(req_mark_read(&id, &token)).await.unwrap();

    assert_eq!(
        res.status(),
        StatusCode::NO_CONTENT,
        "owner must see 204 marking their own notification read (got {:?})",
        res.status()
    );
}

#[tokio::test]
async fn admin_bypass_mark_read_notification() {
    // Admin bypass: a SpacebotAdmin role skips per-resource ownership.
    // Regression guard against `is_admin` returning false on the
    // notifications handler gate.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_notification_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let admin = user_ctx("admin-carol", vec![ROLE_ADMIN]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &admin).await.unwrap();
    let id = seed_notification(&pool, &alice).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&admin);
    let res = app.oneshot(req_mark_read(&id, &token)).await.unwrap();

    assert_eq!(
        res.status(),
        StatusCode::NO_CONTENT,
        "admin must bypass per-resource ownership on \
         POST /notifications/{{id}}/read"
    );
}

#[tokio::test]
async fn non_owner_dismiss_notification_returns_404() {
    // Regression guard for the dismiss write-gate. Shares the inline
    // check_write block with mark_read; if a future refactor drops the
    // gate on dismiss specifically, this test fires.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_notification_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    let id = seed_notification(&pool, &alice).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let res = app.oneshot(req_dismiss(&id, &token)).await.unwrap();

    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "non-owner dismiss on alice's notification must see 404, \
         not 403 or 204"
    );
}

#[tokio::test]
async fn non_owner_list_notifications_by_agent_returns_404() {
    // Regression guard for the list_notifications info-disclosure
    // surface: `?agent_id=<alice-agent>` from Bob must NOT return
    // Alice's notifications. Before this gate landed, a caller could
    // enumerate another user's notifications by passing agent_id
    // directly.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_notification_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    // Seed Alice as the owner of agent-a. Bob will attempt to list by
    // agent_id=agent-a.
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
        .oneshot(req_list_notifications_by_agent("agent-a", &token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "non-owner listing by agent_id must see 404 (hide agent \
         existence), not a notification list"
    );
}

#[tokio::test]
async fn pool_none_skip_mark_read() {
    // Regression guard for the early-startup / static-token fallback
    // path. When instance_pool is not attached, the mark_read gate
    // skips and the handler proceeds to the store lookup (503 because
    // notification_store is also unset without an attached pool).
    // Assertion: NOT 401/403, proving the request passed auth + the
    // no-op authz skip.
    let state = ApiState::new_test_state_with_mock_entra_no_pool();
    let bob = user_ctx("bob", vec![ROLE_USER]);

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let res = app
        .oneshot(req_mark_read("00000000-0000-0000-0000-000000000001", &token))
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
