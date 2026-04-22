//! Phase 5 PR #106 remediation (C4 — wire integration test).
//!
//! The middleware emission added in Task 5.6 (src/auth/middleware.rs) fires
//! `AuditAction::AuthSuccess` or `AuditAction::AuthFailure` per request via
//! a fire-and-forget `tokio::spawn`. The primitive tests in
//! `audit_chain.rs` + `audit_scrubbing.rs` verify that `AuditAppender::append`
//! works; these tests verify the middleware → appender WIRE actually fires.
//!
//! A regression that deletes the spawn block, nulls out the appender handle,
//! or swallows the result silently would pass all primitive tests but leave
//! the SOC 2 CC7.2 evidence trail empty. These integration tests close that
//! gap.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use spacebot::api::ApiState;
use spacebot::api::test_support::build_test_router_entra;
use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::repository::upsert_user_from_auth;
use spacebot::auth::roles::ROLE_USER;
use spacebot::auth::testing::mint_mock_token;
use std::sync::Arc;
use tower::ServiceExt as _;

fn user_ctx(oid: &str) -> AuthContext {
    AuthContext {
        principal_type: PrincipalType::User,
        tid: Arc::from("t1"),
        oid: Arc::from(oid),
        roles: vec![Arc::from(ROLE_USER)],
        groups: vec![],
        groups_overage: false,
        display_email: None,
        display_name: None,
    }
}

/// The spawn is fire-and-forget; give it a moment to land. In practice
/// the append completes sub-ms against an in-memory SQLite, but the task
/// scheduler needs a yield point before the assertion can observe the row.
async fn yield_for_spawned_append() {
    // Two yields beats a `sleep(1ms)` for determinism: tokio::spawn with a
    // ready future schedules on the next poll, and the test-thread's poll
    // happens after the next yield.
    tokio::task::yield_now().await;
    tokio::task::yield_now().await;
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
}

#[tokio::test]
async fn valid_auth_emits_auth_success_row() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user_ctx("alice-auth-success");
    upsert_user_from_auth(&pool, &alice).await.unwrap();

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);

    // Any authenticated route works; hit /api/me since it's the canonical
    // shape-verification endpoint and doesn't require ownership seeding.
    let req = Request::builder()
        .uri("/api/me")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    // Either 200 (if /api/me exists today) or a non-401 status proves auth
    // succeeded. The ROW we care about fires regardless of handler outcome.
    assert_ne!(
        res.status(),
        StatusCode::UNAUTHORIZED,
        "auth should have succeeded with valid mock token"
    );

    yield_for_spawned_append().await;

    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT action, principal_key FROM audit_events WHERE action = 'auth_success'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        !rows.is_empty(),
        "expected at least one auth_success row after valid auth; table was empty — middleware emission may have regressed"
    );
    assert_eq!(rows[0].0, "auth_success");
    assert!(
        rows[0].1.contains("alice-auth-success"),
        "auth_success row should carry the caller's principal_key; got {:?}",
        rows[0].1
    );
}

#[tokio::test]
async fn invalid_auth_emits_auth_failure_row() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;

    let app = build_test_router_entra(state);

    // Malformed bearer token — the mock validator rejects it at the
    // validate step, which drops into the Err(err) arm of
    // entra_auth_middleware where AuditAction::AuthFailure is emitted.
    let req = Request::builder()
        .uri("/api/me")
        .header(header::AUTHORIZATION, "Bearer not-a-valid-token")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::UNAUTHORIZED,
        "invalid token should be rejected"
    );

    yield_for_spawned_append().await;

    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT action, principal_key, result FROM audit_events WHERE action = 'auth_failure'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        !rows.is_empty(),
        "expected at least one auth_failure row after invalid auth; table was empty — middleware emission may have regressed"
    );
    assert_eq!(rows[0].0, "auth_failure");
    assert_eq!(
        rows[0].1, "unknown",
        "auth_failure principal_key should be 'unknown' since the token never validated"
    );
    assert!(
        !rows[0].2.is_empty(),
        "auth_failure result should carry the AuthError::metric_reason() classifier, not empty"
    );
}
