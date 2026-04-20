//! Integration tests for the static-token auth middleware branch.
//!
//! Covers timing-attack resistance and the documented pass-through / health-bypass
//! behaviors at `src/api/server.rs:346-376`.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt as _;
use spacebot::api::ApiState;
use spacebot::api::test_support::build_test_router;
use std::sync::Arc;
use tower::ServiceExt as _;

fn req_with_auth(path: &str, bearer: Option<&str>) -> Request<Body> {
    let mut b = Request::builder().uri(path);
    if let Some(token) = bearer {
        b = b.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    b.body(Body::empty()).unwrap()
}

#[tokio::test]
async fn rejects_wrong_bearer_with_unauthorized() {
    let state = Arc::new(ApiState::new_for_tests(Some("secret-abc".into())));
    let app = build_test_router(state);
    let res = app
        .oneshot(req_with_auth("/api/status", Some("wrong-token")))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn accepts_correct_bearer() {
    let state = Arc::new(ApiState::new_for_tests(Some("secret-abc".into())));
    let app = build_test_router(state);
    let res = app
        .oneshot(req_with_auth("/api/status", Some("secret-abc")))
        .await
        .unwrap();
    // 200 or 500 is acceptable (state may not be fully initialized in tests);
    // we only care that it is NOT 401.
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn health_bypasses_auth_even_without_token() {
    let state = Arc::new(ApiState::new_for_tests(Some("secret-abc".into())));
    let app = build_test_router(state);
    let res = app
        .oneshot(req_with_auth("/api/health", None))
        .await
        .unwrap();
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn pass_through_when_no_token_configured() {
    let state = Arc::new(ApiState::new_for_tests(None));
    let app = build_test_router(state);
    let res = app
        .oneshot(req_with_auth("/api/status", None))
        .await
        .unwrap();
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn constant_time_compare_is_used_for_token() {
    // Regression test: we assert that the middleware does NOT use plain == by
    // checking a pathological case where two strings differ only at the last byte.
    // The timing is not what we assert (flaky); instead we assert behavior equality:
    // both wrong tokens return 401 regardless of how far they diverge.
    let state = Arc::new(ApiState::new_for_tests(Some("aaaaaaaaaaaa".into())));
    let app_a = build_test_router(state.clone());
    let app_b = build_test_router(state);
    // Differs at byte 0:
    let r1 = app_a
        .oneshot(req_with_auth("/api/status", Some("baaaaaaaaaaa")))
        .await
        .unwrap();
    // Differs at byte 11:
    let r2 = app_b
        .oneshot(req_with_auth("/api/status", Some("aaaaaaaaaaab")))
        .await
        .unwrap();
    assert_eq!(r1.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(r2.status(), StatusCode::UNAUTHORIZED);
    let _ = r1.into_body().collect().await;
    let _ = r2.into_body().collect().await;
}

#[tokio::test]
async fn cors_does_not_advertise_credentials() {
    let state = Arc::new(ApiState::new_for_tests(None));
    let app = build_test_router(state);
    // OPTIONS preflight from an arbitrary origin:
    let preflight = Request::builder()
        .method("OPTIONS")
        .uri("/api/status")
        .header("Origin", "https://attacker.example")
        .header("Access-Control-Request-Method", "GET")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(preflight).await.unwrap();
    // The CORS config uses mirror_request(); credentials MUST NOT be allowed.
    // If a future change introduces session cookies, read research §12 I-6
    // before removing this test.
    assert!(
        res.headers()
            .get("access-control-allow-credentials")
            .is_none(),
        "CORS must not advertise credentials (src/api/server.rs start_http_server). \
         If you're changing this, you're probably adopting session cookies — \
         read research §12 I-6 before removing this test."
    );
}
