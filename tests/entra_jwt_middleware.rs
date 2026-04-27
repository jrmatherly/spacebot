//! Phase 1 Task 1.12: Wiremock-backed integration tests for the Entra JWT
//! validator. Covers the happy path, the wrong-signature path, and the
//! service-principal path.

#[path = "support/mock_entra.rs"]
mod mock_entra;

use mock_entra::MockTenant;
use std::sync::Arc;

fn cfg_for(tenant: &MockTenant) -> spacebot::auth::EntraAuthConfig {
    // Construct via the doc-hidden test constructor. `EntraAuthConfig` now
    // has `pub(crate)` override fields so this is the only way external
    // tests can point the validator at a Wiremock-backed tenant.
    spacebot::auth::EntraAuthConfig::new_for_test(
        Arc::from(tenant.tenant_id.as_str()),
        Arc::from(tenant.audience.as_str()),
        vec!["api.access".into()],
        Arc::from("test-spa"),
        vec![Arc::from("api://test/api.access")],
    )
    .with_test_overrides(tenant.jwks_url(), tenant.issuer())
}

#[tokio::test]
async fn mock_tenant_serves_jwks() {
    let tenant = MockTenant::start().await;
    let response = reqwest::get(tenant.jwks_url()).await.unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["keys"][0]["kid"], "test-kid-1");
    assert_eq!(body["keys"][0]["alg"], "RS256");
}

#[tokio::test]
async fn validator_accepts_valid_user_token() {
    let tenant = MockTenant::start().await;
    let cfg = cfg_for(&tenant);
    let validator = spacebot::auth::EntraValidator::new(cfg)
        .await
        .expect("validator init");

    let token = tenant.mint_user_token("user-oid-1", &["SpacebotUser"], &["group-a"]);
    let ctx = validator.validate(&token).await.expect("valid token");

    assert_eq!(
        ctx.principal_type,
        spacebot::auth::PrincipalType::User,
        "delegated (scp-bearing) tokens classify as User"
    );
    assert_eq!(&*ctx.oid, "user-oid-1");
    assert_eq!(&*ctx.tid, tenant.tenant_id.as_str());
    assert!(ctx.has_role("SpacebotUser"));
    assert!(!ctx.has_role("SpacebotAdmin"));
    assert_eq!(ctx.groups.len(), 1);
    assert_eq!(&*ctx.groups[0], "group-a");
    assert!(!ctx.groups_overage);
    assert_eq!(
        ctx.display_email.as_deref().map(|s| s.to_string()),
        Some("user-oid-1@example.com".to_string())
    );
}

#[tokio::test]
async fn validator_rejects_wrong_signature() {
    let tenant = MockTenant::start().await;
    let cfg = cfg_for(&tenant);
    let validator = spacebot::auth::EntraValidator::new(cfg)
        .await
        .expect("validator init");

    let token = tenant.mint_wrong_sig_token("user-oid-2");
    let err = validator.validate(&token).await.expect_err("must reject");
    // Wrong signature is an InvalidToken (401), not a JwksUnreachable (503).
    assert_eq!(err.status(), axum::http::StatusCode::UNAUTHORIZED);
    assert_eq!(err.metric_reason(), "invalid_token");
}

#[tokio::test]
async fn validator_rejects_empty_token_string() {
    // The previous name (`..._via_middleware`) was misleading: this test
    // calls the validator directly. Middleware-level bearer-parsing tests
    // live in the router-integration suite below.
    let tenant = MockTenant::start().await;
    let cfg = cfg_for(&tenant);
    let validator = spacebot::auth::EntraValidator::new(cfg)
        .await
        .expect("validator init");

    let err = validator.validate("").await.expect_err("must reject");
    assert_eq!(err.status(), axum::http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn validator_accepts_service_principal_with_roles() {
    let tenant = MockTenant::start().await;
    let cfg = cfg_for(&tenant);
    let validator = spacebot::auth::EntraValidator::new(cfg)
        .await
        .expect("validator init");

    let token = tenant.mint_service_principal_token("sp-oid-1", &["SpacebotService"]);
    let ctx = validator.validate(&token).await.expect("valid SP token");

    assert_eq!(
        ctx.principal_type,
        spacebot::auth::PrincipalType::ServicePrincipal,
        "tokens without `scp` claim classify as ServicePrincipal"
    );
    assert!(ctx.has_role("SpacebotService"));
    assert_eq!(&*ctx.oid, "sp-oid-1");
}

#[tokio::test]
async fn validator_rejects_service_principal_with_no_roles() {
    let tenant = MockTenant::start().await;
    let cfg = cfg_for(&tenant);
    let validator = spacebot::auth::EntraValidator::new(cfg)
        .await
        .expect("validator init");

    let token = tenant.mint_service_principal_token("sp-oid-2", &[]);
    let err = validator
        .validate(&token)
        .await
        .expect_err("SP with no roles must be rejected");
    // Forbidden (403), not Unauthorized (401): the token is syntactically
    // valid but the principal lacks app roles.
    assert_eq!(err.status(), axum::http::StatusCode::FORBIDDEN);
    assert_eq!(err.metric_reason(), "forbidden");
}

#[tokio::test]
async fn validator_rejects_user_with_wrong_scope() {
    let tenant = MockTenant::start().await;
    let cfg = cfg_for(&tenant);
    let validator = spacebot::auth::EntraValidator::new(cfg)
        .await
        .expect("validator init");

    // User token with an unrelated scope (e.g., a Microsoft Graph token
    // pasted into a Spacebot request).
    let token = tenant.mint_user_token_with_scope("user-oid-3", "user.read");
    let err = validator
        .validate(&token)
        .await
        .expect_err("wrong scope must be rejected");
    assert_eq!(err.status(), axum::http::StatusCode::FORBIDDEN);
    assert_eq!(err.metric_reason(), "forbidden");
}

#[tokio::test]
async fn validator_rejects_expired_token() {
    let tenant = MockTenant::start().await;
    let cfg = cfg_for(&tenant);
    let validator = spacebot::auth::EntraValidator::new(cfg)
        .await
        .expect("validator init");

    let token = tenant.mint_expired_token("user-oid-4");
    let err = validator
        .validate(&token)
        .await
        .expect_err("expired token must be rejected");
    assert_eq!(err.status(), axum::http::StatusCode::UNAUTHORIZED);
    assert_eq!(err.metric_reason(), "temporal_invalid");
}

#[tokio::test]
async fn validator_rejects_not_yet_valid_token() {
    let tenant = MockTenant::start().await;
    let cfg = cfg_for(&tenant);
    let validator = spacebot::auth::EntraValidator::new(cfg)
        .await
        .expect("validator init");

    let token = tenant.mint_not_yet_valid_token("user-oid-5");
    let err = validator
        .validate(&token)
        .await
        .expect_err("nbf-in-future token must be rejected");
    assert_eq!(err.status(), axum::http::StatusCode::UNAUTHORIZED);
    assert_eq!(err.metric_reason(), "temporal_invalid");
}

#[tokio::test]
async fn validator_rejects_wrong_audience() {
    let tenant = MockTenant::start().await;
    let cfg = cfg_for(&tenant);
    let validator = spacebot::auth::EntraValidator::new(cfg)
        .await
        .expect("validator init");

    let token = tenant.mint_token_with_aud("user-oid-6", "api://other-app");
    let err = validator
        .validate(&token)
        .await
        .expect_err("wrong audience must be rejected");
    assert_eq!(err.status(), axum::http::StatusCode::UNAUTHORIZED);
    assert_eq!(err.metric_reason(), "invalid_token");
}

#[tokio::test]
async fn validator_rejects_wrong_issuer() {
    let tenant = MockTenant::start().await;
    let cfg = cfg_for(&tenant);
    let validator = spacebot::auth::EntraValidator::new(cfg)
        .await
        .expect("validator init");

    let token = tenant.mint_token_with_iss(
        "user-oid-7",
        "https://login.microsoftonline.com/evil-tenant/v2.0",
    );
    let err = validator
        .validate(&token)
        .await
        .expect_err("wrong issuer must be rejected");
    assert_eq!(err.status(), axum::http::StatusCode::UNAUTHORIZED);
    assert_eq!(err.metric_reason(), "invalid_token");
}

// ---------------------------------------------------------------------------
// Router-level middleware integration tests.
//
// These drive the middleware in-process via `tower::ServiceExt::oneshot`,
// matching the pattern in `tests/api_auth_middleware.rs`. Covers branches
// that pure validator-level tests don't reach: bearer header parsing,
// validator-absent 500, response body shape, status propagation.
// ---------------------------------------------------------------------------

mod router_level {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use spacebot::api::ApiState;
    use spacebot::api::test_support::build_test_router_entra;
    use tower::ServiceExt as _;

    async fn router_for(tenant: &MockTenant) -> axum::Router {
        let state = Arc::new(ApiState::new_for_tests(None));
        let cfg = cfg_for(tenant);
        let validator = spacebot::auth::EntraValidator::new(cfg)
            .await
            .expect("validator init");
        state.set_entra_auth(Arc::new(validator));
        build_test_router_entra(state)
    }

    fn req_with_auth(path: &str, bearer: Option<&str>) -> Request<Body> {
        let mut b = Request::builder().uri(path);
        if let Some(token) = bearer {
            b = b.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        b.body(Body::empty()).unwrap()
    }

    fn req_with_raw_auth(path: &str, raw_auth: &str) -> Request<Body> {
        Request::builder()
            .uri(path)
            .header(header::AUTHORIZATION, raw_auth)
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn middleware_rejects_missing_authorization_header() {
        let tenant = MockTenant::start().await;
        let app = router_for(&tenant).await;
        let res = app
            .oneshot(req_with_auth("/api/status", None))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    /// Phase 6 C3 remediation: lock the Entra-JWT middleware allowlist
    /// entry for `/api/auth/config`. Mirrors
    /// `auth_config_bypasses_token_check` in `tests/api_auth_middleware.rs`
    /// (which covers the static-token branch). The allowlist is a single
    /// path literal hand-maintained across the two middleware branches —
    /// a future refactor that drops it from one branch would only be
    /// caught if both branches have a regression test.
    #[tokio::test]
    async fn auth_config_bypasses_entra_jwt_check() {
        let tenant = MockTenant::start().await;
        let app = router_for(&tenant).await;
        let res = app
            .oneshot(req_with_auth("/api/auth/config", None))
            .await
            .unwrap();
        // No Authorization header and no valid JWT — must still reach the
        // handler. Critical assertion is "not 401"; the 200 follow-up
        // tightens against regressions that surface as 500/404.
        assert_ne!(
            res.status(),
            StatusCode::UNAUTHORIZED,
            "/api/auth/config must bypass the Entra JWT check"
        );
        assert_eq!(res.status(), StatusCode::OK);
    }

    /// Phase 8 Task 8.A.4 — Entra-JWT branch counterpart of
    /// `desktop_tokens_bypasses_token_check` in
    /// `tests/api_auth_middleware.rs`. Same obligation (both middleware
    /// branches must honor the allowlist) enforced on the second branch.
    #[tokio::test]
    async fn desktop_tokens_bypasses_entra_jwt_check() {
        let tenant = MockTenant::start().await;
        let app = router_for(&tenant).await;
        let req = Request::builder()
            .method("POST")
            .uri("/api/desktop/tokens")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_ne!(
            res.status(),
            StatusCode::UNAUTHORIZED,
            "/api/desktop/tokens must bypass the Entra JWT check"
        );
    }

    #[tokio::test]
    async fn middleware_rejects_non_bearer_scheme() {
        let tenant = MockTenant::start().await;
        let app = router_for(&tenant).await;
        let res = app
            .oneshot(req_with_raw_auth(
                "/api/status",
                "Basic dXNlcjpwYXNzd29yZA==",
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn middleware_rejects_non_ascii_authorization_header() {
        let tenant = MockTenant::start().await;
        let app = router_for(&tenant).await;
        // Build a Request with a non-ASCII byte in the Authorization value.
        // `HeaderValue::from_bytes` accepts the bytes. `.to_str()` then
        // fails at middleware time and maps to MalformedHeader.
        let mut req = Request::builder()
            .uri("/api/status")
            .body(Body::empty())
            .unwrap();
        let bad = axum::http::HeaderValue::from_bytes(b"Bearer \xff\xfe")
            .expect("construct non-ASCII header value");
        req.headers_mut().insert(header::AUTHORIZATION, bad);
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn middleware_rejects_bearer_with_bad_token() {
        let tenant = MockTenant::start().await;
        let app = router_for(&tenant).await;
        let res = app
            .oneshot(req_with_auth("/api/status", Some("not.a.jwt")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn middleware_bypasses_auth_for_health_endpoint() {
        let tenant = MockTenant::start().await;
        let app = router_for(&tenant).await;
        // `/api/health` is bypassed by the middleware and routes to a real
        // handler that returns 200. Asserting 200 (not just "not 401")
        // catches regressions where the bypass is removed AND the handler
        // path fails with a 5xx at the same time.
        let res = app
            .oneshot(req_with_auth("/api/health", None))
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            StatusCode::OK,
            "health bypass must skip auth and reach the 200 handler"
        );
    }

    /// A-10 (Phase 3 Task 3.3b): when a freshly authenticated user hits the
    /// daemon for the first time and the token's `groups` claim is
    /// non-empty (so the middleware knows memberships SHOULD exist) but
    /// `team_memberships` has no rows yet, the middleware returns
    /// `202 Accepted` with `Retry-After: 2` so the SPA can retry instead
    /// of surfacing spurious 404s from Phase 4 team-scoped resources.
    #[tokio::test]
    async fn returns_202_when_memberships_not_yet_synced() {
        use sqlx::sqlite::SqlitePoolOptions;

        let tenant = MockTenant::start().await;
        let state = Arc::new(ApiState::new_for_tests(None));
        let cfg = cfg_for(&tenant);
        let validator = spacebot::auth::EntraValidator::new(cfg)
            .await
            .expect("validator init");
        state.set_entra_auth(Arc::new(validator));

        // Stand up an in-memory instance pool with the Phase 2+3 schema.
        // The middleware reads team_memberships on this pool; without it
        // the 202 race branch can't fire.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite");
        sqlx::migrate!("./migrations/global")
            .run(&pool)
            .await
            .expect("global migrations");
        state.set_instance_pool(std::sync::Arc::new(spacebot::db::DbPool::Sqlite(pool)));

        let app = build_test_router_entra(state);

        // Mint a user token that claims one group. This satisfies the
        // middleware's `expect_memberships = !ctx.groups.is_empty()` half
        // of the 202 predicate. No row exists in team_memberships yet, so
        // the middleware should return 202 instead of routing through.
        let token = tenant.mint_user_token("oid-first-req", &[], &["grp-xyz"]);

        let res = app
            .oneshot(req_with_auth("/api/status", Some(&token)))
            .await
            .unwrap();

        assert_eq!(
            res.status(),
            StatusCode::ACCEPTED,
            "first request with expected memberships but no rows should return 202",
        );
        let retry_after = res
            .headers()
            .get("retry-after")
            .expect("Retry-After header")
            .to_str()
            .expect("ascii")
            .to_string();
        assert_eq!(retry_after, "2", "Retry-After should be 2 seconds");
    }
}
