//! Phase 10 Task 10.4: JWKS rotation regression test (SOC 2 evidence).
//! When Entra rotates its signing keys, the daemon's JWKS cache may hold
//! the previous key set. The validator must detect an unknown `kid` on
//! an incoming token and refetch JWKS rather than rejecting valid tokens
//! signed by the new key. This test pins that contract end-to-end with
//! a wiremock-backed `MockTenant` whose keys can be rotated mid-test.
//!
//! Per A-07: pure-Rust `rsa` crate, no `openssl` system dep.
//! Per the Phase 10 plan: uses `MockTenant::mint_user_token` directly,
//! not the `mint_mock_token` helper paired with `MockValidator`.

#[path = "support/mock_entra.rs"]
mod mock_entra;

use mock_entra::MockTenant;
use std::sync::Arc;

fn cfg_for(tenant: &MockTenant) -> spacebot::auth::EntraAuthConfig {
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
async fn validator_refetches_jwks_on_unknown_kid() {
    let mut tenant = MockTenant::start().await;
    let cfg = cfg_for(&tenant);
    let validator = spacebot::auth::EntraValidator::new(cfg)
        .await
        .expect("validator init");

    // First token signed with the original kid: validator caches that key.
    let t1 = tenant.mint_user_token("alice", &[], &[]);
    let ctx1 = validator
        .validate(&t1)
        .await
        .expect("original-kid token validates");
    assert_eq!(ctx1.oid.as_ref(), "alice");

    // Rotate keys at the tenant. The wiremock JWKS endpoint now returns a
    // fresh key set under a new kid; the validator's cached keys reference
    // the old kid only.
    tenant.rotate_keys().await;
    let t2 = tenant.mint_user_token("bob", &[], &[]);

    // The validator must encounter the unknown kid, refetch JWKS, find the
    // new key, and validate the token. If JWKS-refetch is broken, this
    // returns an InvalidToken error.
    let ctx2 = validator
        .validate(&t2)
        .await
        .expect("post-rotation token validates after JWKS refetch");
    assert_eq!(ctx2.oid.as_ref(), "bob");
}
