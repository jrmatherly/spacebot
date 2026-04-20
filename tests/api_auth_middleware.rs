//! Integration tests for the static-token auth middleware branch.
//!
//! Covers timing-attack resistance and the documented pass-through / health-bypass
//! behaviors of `api_auth_middleware` in `src/api/server.rs`.

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

/// Build a request with a raw `Authorization` header (no auto-prefix).
/// Lets tests assert behavior on malformed or non-Bearer schemes.
fn req_with_raw_auth(path: &str, raw_auth: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .header(header::AUTHORIZATION, raw_auth)
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn rejects_wrong_bearer_with_unauthorized() {
    let state = Arc::new(ApiState::new_for_tests(Some("secret-abc".into())));
    let app = build_test_router(state);

    // Baseline the counter before the request so the delta assertion below
    // is robust to parallel tests that also emit static_token failures.
    #[cfg(feature = "metrics")]
    let before = spacebot::telemetry::Metrics::global()
        .auth_failures_total
        .with_label_values(&["static_token", "token_mismatch"])
        .get();

    let res = app
        .oneshot(req_with_auth("/api/status", Some("wrong-token")))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    #[cfg(feature = "metrics")]
    {
        let after = spacebot::telemetry::Metrics::global()
            .auth_failures_total
            .with_label_values(&["static_token", "token_mismatch"])
            .get();
        // `Metrics::global()` is process-wide and cargo test runs tests in
        // parallel by default, so the three other rejection-path tests
        // (`rejects_non_bearer_scheme`, `rejects_bearer_without_space_separator`,
        // `rejects_empty_bearer_token`) can each emit their own reasons and
        // the token_mismatch reason could also be incremented by any
        // concurrent test that exercises that path. We only assert the delta
        // is at least 1, which is sufficient to catch a regression where
        // `.inc()` is dropped, mislabeled, or moved out of the 401 branch.
        assert!(
            after > before,
            "auth_failures_total{{branch=static_token,reason=token_mismatch}} \
             did not increment on a rejected bearer (before={before}, after={after})",
        );
    }
}

#[tokio::test]
async fn accepts_correct_bearer() {
    let state = Arc::new(ApiState::new_for_tests(Some("secret-abc".into())));
    let app = build_test_router(state);
    let res = app
        .oneshot(req_with_auth("/api/status", Some("secret-abc")))
        .await
        .unwrap();
    // `/api/status` returns `Json<StatusResponse>` unconditionally, so a
    // matching bearer must produce 200. Tightened from `assert_ne!(401)`
    // to catch regressions that return 500/404 from an unrelated auth path.
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn health_bypasses_auth_even_without_token() {
    let state = Arc::new(ApiState::new_for_tests(Some("secret-abc".into())));
    let app = build_test_router(state);
    let res = app
        .oneshot(req_with_auth("/api/health", None))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn pass_through_when_no_token_configured() {
    let state = Arc::new(ApiState::new_for_tests(None));
    let app = build_test_router(state);
    let res = app
        .oneshot(req_with_auth("/api/status", None))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn rejects_wrong_token_regardless_of_divergence_point() {
    // Behavioral regression guard. We cannot assert constant-time execution
    // (timing measurement is flaky under load). What we CAN assert is that
    // two wrong tokens diverging at opposite ends of the string both return
    // 401. This pins the middleware to treating the token as opaque rather
    // than short-circuiting at the first mismatched byte in a way that
    // surfaces structurally (e.g. 404 for one token, 401 for the other).
    //
    // The load-bearing defense is `subtle::ConstantTimeEq::ct_eq` at the
    // call site in `api_auth_middleware` and the `use subtle::ConstantTimeEq;`
    // import. Reverting to `==` would still pass this test (same 401 outcome),
    // so this is not a `ct_eq` proof. The test name was historically
    // `constant_time_compare_is_used_for_token` which overclaimed — renamed
    // to match the actual invariant guarded.
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
async fn rejects_request_with_no_authorization_header() {
    let state = Arc::new(ApiState::new_for_tests(Some("secret-abc".into())));
    let app = build_test_router(state);
    let req = Request::builder()
        .uri("/api/status")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn rejects_non_bearer_scheme() {
    let state = Arc::new(ApiState::new_for_tests(Some("secret-abc".into())));
    let app = build_test_router(state);
    // `Basic ` is a valid HTTP auth scheme but not what the middleware
    // accepts. Must still 401, not pass through or 500.
    let res = app
        .oneshot(req_with_raw_auth("/api/status", "Basic dXNlcjpwYXNz"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn rejects_bearer_without_space_separator() {
    let state = Arc::new(ApiState::new_for_tests(Some("secret-abc".into())));
    let app = build_test_router(state);
    // `Bearersecret-abc` must not match the `Bearer ` prefix (note the
    // required trailing space). A greedy `contains`-style check would
    // pass this; `strip_prefix` correctly rejects it.
    let res = app
        .oneshot(req_with_raw_auth("/api/status", "Bearersecret-abc"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn rejects_empty_bearer_token() {
    let state = Arc::new(ApiState::new_for_tests(Some("secret-abc".into())));
    let app = build_test_router(state);
    // `Bearer ` with an empty token must 401, not match the configured
    // `secret-abc`. Guards against a regression where `strip_prefix`
    // returning `Some("")` somehow compares equal to a non-empty expected.
    let res = app
        .oneshot(req_with_raw_auth("/api/status", "Bearer "))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
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
        "CORS must not advertise credentials (see `start_http_server`). \
         If you're changing this, you're probably adopting session cookies. \
         Read research §12 I-6 before removing this test."
    );
    // Positive assertion: the CorsLayer is actually engaged. If
    // `build_test_router` regressed to drop the layer, the preflight would
    // respond without `access-control-allow-origin`, and the credentials
    // check above would still pass (absence on absence). Requiring the
    // origin header proves the layer is wired.
    let origin = res
        .headers()
        .get("access-control-allow-origin")
        .expect("CORS layer must echo the mirrored origin on preflight");
    assert_eq!(origin.to_str().unwrap(), "https://attacker.example");
}
