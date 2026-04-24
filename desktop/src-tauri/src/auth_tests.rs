//! Unit tests for the Entra authorization-request helpers.
//!
//! Sibling file to `auth.rs` (not nested under it), so imports use
//! `crate::auth::*` rather than `super::auth`.

use crate::auth::{build_authorize_url, generate_state, AuthorizeParams};

#[test]
fn state_is_url_safe_and_non_empty() {
    let s = generate_state();
    assert!(!s.is_empty());
    assert!(s.len() >= 32, "state must be at least 32 chars for entropy");
    for b in s.bytes() {
        assert!(
            b.is_ascii_alphanumeric() || b == b'-' || b == b'_',
            "state must be URL-safe: got byte {b}"
        );
    }
}

#[test]
fn generated_states_are_unique() {
    let a = generate_state();
    let b = generate_state();
    assert_ne!(a, b, "fresh states must differ (CSPRNG output)");
}

#[test]
fn authorize_url_includes_all_required_params() {
    let url = build_authorize_url(&AuthorizeParams {
        tenant_id: "tenant-1",
        client_id: "client-abc",
        redirect_uri: "http://127.0.0.1:50001/callback",
        scopes: &["api://web-api/api.access".into(), "offline_access".into()],
        state: "state-xyz",
        code_challenge: "pkce-chal",
    });
    let parsed = url::Url::parse(&url).unwrap();
    assert!(parsed.host_str().unwrap().contains("login.microsoftonline.com"));
    assert!(parsed.path().contains("tenant-1/oauth2/v2.0/authorize"));
    let q: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
    assert_eq!(q.get("client_id").map(|s| s.as_str()), Some("client-abc"));
    assert_eq!(
        q.get("redirect_uri").map(|s| s.as_str()),
        Some("http://127.0.0.1:50001/callback")
    );
    assert_eq!(q.get("response_type").map(|s| s.as_str()), Some("code"));
    assert_eq!(q.get("state").map(|s| s.as_str()), Some("state-xyz"));
    assert_eq!(q.get("code_challenge").map(|s| s.as_str()), Some("pkce-chal"));
    assert_eq!(q.get("code_challenge_method").map(|s| s.as_str()), Some("S256"));
    assert!(q.get("scope").unwrap().contains("api.access"));
}
