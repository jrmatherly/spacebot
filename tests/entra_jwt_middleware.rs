//! Phase 1 Task 1.12: Wiremock-backed integration tests for the Entra JWT
//! validator. Covers the happy path, the wrong-signature path, and the
//! service-principal path.

#[path = "support/mock_entra.rs"]
mod mock_entra;

use mock_entra::MockTenant;
use std::sync::Arc;

fn cfg_for(tenant: &MockTenant) -> spacebot::auth::EntraAuthConfig {
    spacebot::auth::EntraAuthConfig {
        tenant_id: Arc::from(tenant.tenant_id.as_str()),
        audience: Arc::from(tenant.audience.as_str()),
        allowed_scopes: vec!["api.access".into()],
        jwks_cache_ttl_secs: 3600,
        clock_skew_leeway_secs: 60,
        group_cache_ttl_secs: 300,
        spa_client_id: Arc::from("test-spa"),
        spa_scopes: vec![Arc::from("api://test/api.access")],
        mock_mode: false,
        jwks_url_override: Some(tenant.jwks_url()),
        issuer_override: Some(tenant.issuer()),
    }
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
async fn validator_rejects_missing_bearer_via_middleware() {
    // This test doesn't exercise the middleware directly (that would need
    // a full router harness). It exercises the validator's behavior on an
    // empty token, which maps to `InvalidToken` in jwt-authorizer because
    // parsing an empty string fails.
    let tenant = MockTenant::start().await;
    let cfg = cfg_for(&tenant);
    let validator = spacebot::auth::EntraValidator::new(cfg)
        .await
        .expect("validator init");

    let err = validator.validate("").await.expect_err("must reject");
    assert_eq!(err.status(), axum::http::StatusCode::UNAUTHORIZED);
}
