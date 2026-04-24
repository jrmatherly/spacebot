//! Integration tests for `GET /api/desktop/tokens` — the loopback-only
//! cached-token-read endpoint that lets the Tauri desktop seed MSAL on
//! cold start without re-running the system-browser sign-in flow.
//!
//! Mirrors `tests/api_desktop_tokens.rs` for the POST sibling. Same
//! three-layer defense (peer IP, Host header, locked-store 503), plus
//! a happy-path assertion that the response shape matches the
//! `DesktopTokenStatus` schema.

use axum::Router;
use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt as _;
use spacebot::api::ApiState;
use spacebot::api::test_support::build_test_router;
use spacebot::secrets::store::{SecretCategory, SecretsStore};
use std::net::SocketAddr;
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt as _;

fn router_with_peer(
    peer: SocketAddr,
    secrets: Option<Arc<SecretsStore>>,
) -> (Router, Arc<ApiState>) {
    let state = Arc::new(ApiState::new_for_tests(Some("test-token".into())));
    if let Some(store) = secrets {
        state.set_secrets_store(store);
    }
    let app = build_test_router(state.clone()).layer(MockConnectInfo(peer));
    (app, state)
}

fn get_tokens(host: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri("/api/desktop/tokens")
        .header(header::HOST, host)
        .body(Body::empty())
        .unwrap()
}

fn fresh_unlocked_store() -> (Arc<SecretsStore>, TempDir) {
    let dir = TempDir::new().expect("temp dir");
    let store = SecretsStore::new(dir.path().join("secrets.redb")).expect("open store");
    (Arc::new(store), dir)
}

fn fresh_locked_store() -> (Arc<SecretsStore>, TempDir) {
    let dir = TempDir::new().expect("temp dir");
    let store = SecretsStore::new(dir.path().join("secrets.redb")).expect("open store");
    let _master_key = store.enable_encryption().expect("enable encryption");
    store.lock().expect("lock store");
    (Arc::new(store), dir)
}

// ----------------------------------------------------------------------------
// Peer-IP loopback gate (three-layer defense, layer 1)
// ----------------------------------------------------------------------------

#[tokio::test]
async fn rejects_non_loopback_peer() {
    let peer: SocketAddr = "10.0.0.1:12345".parse().unwrap();
    let (store, _dir) = fresh_unlocked_store();
    let (app, _state) = router_with_peer(peer, Some(store));
    let res = app.oneshot(get_tokens("127.0.0.1")).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "GET /api/desktop/tokens must reject non-loopback peers"
    );
}

// ----------------------------------------------------------------------------
// Host-header pin (three-layer defense, layer 2: DNS-rebinding guard)
// ----------------------------------------------------------------------------

#[tokio::test]
async fn rejects_attacker_host_header() {
    let peer: SocketAddr = "127.0.0.1:54321".parse().unwrap();
    let (store, _dir) = fresh_unlocked_store();
    let (app, _state) = router_with_peer(peer, Some(store));
    let res = app.oneshot(get_tokens("attacker.example")).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "GET /api/desktop/tokens must reject DNS-rebinding Host"
    );
}

// ----------------------------------------------------------------------------
// Locked-store surfacing (three-layer defense, layer 3)
// ----------------------------------------------------------------------------

#[tokio::test]
async fn returns_503_when_store_locked() {
    let peer: SocketAddr = "127.0.0.1:54321".parse().unwrap();
    let (store, _dir) = fresh_locked_store();
    let (app, _state) = router_with_peer(peer, Some(store));
    let res = app.oneshot(get_tokens("127.0.0.1")).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "locked store on GET must surface as 503"
    );
}

// ----------------------------------------------------------------------------
// Absent token returns 200 with null access_token (NOT 404)
// ----------------------------------------------------------------------------

#[tokio::test]
async fn returns_null_when_no_token_persisted() {
    let peer: SocketAddr = "127.0.0.1:54321".parse().unwrap();
    let (store, _dir) = fresh_unlocked_store();
    let (app, _state) = router_with_peer(peer, Some(store));
    let res = app.oneshot(get_tokens("127.0.0.1")).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let parsed: serde_json::Value = serde_json::from_slice(&body).expect("valid json");
    assert!(
        parsed.get("access_token").is_some_and(|v| v.is_null()),
        "absent token must serialize as access_token: null, got {parsed}"
    );
    assert!(
        parsed
            .get("expires_in_epoch")
            .is_some_and(|v| v.is_null()),
        "expires_in_epoch must be null today (no persistence yet)"
    );
}

// ----------------------------------------------------------------------------
// Happy path — cached token returned verbatim
// ----------------------------------------------------------------------------

#[tokio::test]
async fn returns_cached_access_token() {
    let peer: SocketAddr = "127.0.0.1:54321".parse().unwrap();
    let (store, _dir) = fresh_unlocked_store();
    store
        .set("entra_access_token", "cached-jwt", SecretCategory::System)
        .expect("seed access token");
    let (app, _state) = router_with_peer(peer, Some(store));
    let res = app.oneshot(get_tokens("127.0.0.1")).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let parsed: serde_json::Value = serde_json::from_slice(&body).expect("valid json");
    assert_eq!(
        parsed.get("access_token").and_then(|v| v.as_str()),
        Some("cached-jwt"),
        "GET must return the persisted access token verbatim"
    );
}
