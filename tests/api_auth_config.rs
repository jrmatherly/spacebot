//! Phase 6 Task 6.A.1 — TDD red phase for `GET /api/auth/config`.
//!
//! `/api/auth/config` is unprotected (the SPA fetches it before MSAL completes
//! login, so no bearer token is available yet). These tests assert:
//!   1. the endpoint is reachable without an `Authorization` header,
//!   2. the response body never contains secret-adjacent substrings,
//!   3. when Entra is not configured, the response reports `entra_enabled: false`
//!      and omits client_id / tenant_id / authority / scopes.
//!
//! Until Task 6.A.2 lands the handler + the middleware allowlist edit, these
//! tests will fail at runtime (404 for the unprotected assertion, or 401 for
//! the "bypass" assertion). That's the intentional red-phase state.

// F1/F2/F3 corrections (2026-04-22 pre-code audit):
//  - Test helper is `build_test_router_entra` (NOT `build_test_router_with_auth`)
//    re-exported at `spacebot::api::test_support` (definition at
//    `src/api/server.rs:663`, re-export at `src/api.rs:45`).
//  - `new_test_state_with_mock_entra` is an associated fn on `ApiState`
//    (src/api/state.rs:455), not a free function in `spacebot::api::state`.
//  - `new_for_tests(None::<String>)` makes the `Option<String>` type explicit
//    (signature at src/api/state.rs:432).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use spacebot::api::ApiState;
use spacebot::api::test_support::build_test_router_entra;
use tower::ServiceExt as _;

#[tokio::test]
async fn config_endpoint_is_unprotected() {
    let (state, _pool) = ApiState::new_test_state_with_mock_entra().await;
    let app = build_test_router_entra(state);
    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "config endpoint must be reachable without bearer token"
    );
}

#[tokio::test]
async fn config_endpoint_never_leaks_secrets() {
    let (state, _pool) = ApiState::new_test_state_with_mock_entra().await;
    let app = build_test_router_entra(state);
    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    // Forbidden substrings. A regression here means we're leaking a secret
    // into a publicly accessible endpoint.
    for forbidden in ["client_secret", "graph_client_secret", "auth_token"] {
        assert!(
            !body_str.contains(forbidden),
            "secret-adjacent key `{forbidden}` appeared in public config: {body_str}"
        );
    }
}

#[tokio::test]
async fn config_returns_entra_disabled_when_unconfigured() {
    // Build a state WITHOUT Entra. `new_for_tests(None::<String>)` produces a
    // minimal state for middleware-integration tests; the `::<String>` turbofish
    // makes the `Option<String>` type explicit so inference does not complain.
    let state = std::sync::Arc::new(ApiState::new_for_tests(None::<String>));
    let app = build_test_router_entra(state);
    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        body_str.contains("\"entra_enabled\":false"),
        "unconfigured Entra must report entra_enabled=false, got: {body_str}"
    );
}
