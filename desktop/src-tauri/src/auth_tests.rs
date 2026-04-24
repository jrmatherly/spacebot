//! Unit tests for the Entra authorization-request helpers.

use crate::auth::{
    AuthorizeParams, DESKTOP_PORT_RANGE, bind_loopback, build_authorize_url, generate_pkce,
    generate_state,
};

#[test]
fn state_is_at_least_32_chars() {
    let s = generate_state();
    assert!(
        s.len() >= 32,
        "state must be at least 32 chars for entropy; got {}",
        s.len()
    );
}

#[test]
fn state_is_url_safe_base64() {
    let s = generate_state();
    assert!(!s.is_empty());
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
fn pkce_challenge_is_s256_of_verifier() {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    let (verifier, challenge) = generate_pkce();
    assert!(
        verifier.len() >= 43,
        "PKCE verifier must be at least 43 chars per RFC 7636; got {}",
        verifier.len()
    );
    let expected =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    assert_eq!(challenge, expected, "challenge must equal S256(verifier)");
}

#[test]
fn bind_loopback_returns_port_in_documented_range() {
    let (listener, port) = bind_loopback().expect("bind loopback on a free range port");
    assert!(
        DESKTOP_PORT_RANGE.contains(&port),
        "bound port {port} must fall inside DESKTOP_PORT_RANGE {:?}",
        DESKTOP_PORT_RANGE
    );
    drop(listener);
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
    // Scope must be space-joined verbatim. A refactor that joins with `,`
    // or URL-double-encodes would pass `contains("api.access")` but break
    // auth, so pin the exact string instead.
    assert_eq!(
        q.get("scope").map(|s| s.as_str()),
        Some("api://web-api/api.access offline_access")
    );
}
