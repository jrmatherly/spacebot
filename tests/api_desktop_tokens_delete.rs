//! Integration tests for `DELETE /api/desktop/tokens` — the
//! loopback-only sign-out endpoint that wipes both `entra_access_token`
//! and `entra_refresh_token` from the daemon's `SecretsStore`.
//!
//! Mirrors `tests/api_desktop_tokens.rs` for the POST sibling. Same
//! three-layer defense (peer IP, Host header, locked-store 503), plus
//! an idempotency assertion: DELETE on an absent key still returns 204.

use axum::Router;
use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode, header};
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

fn delete_tokens(host: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
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
// Peer-IP loopback gate
// ----------------------------------------------------------------------------

#[tokio::test]
async fn rejects_non_loopback_peer() {
    let peer: SocketAddr = "10.0.0.1:12345".parse().unwrap();
    let (store, _dir) = fresh_unlocked_store();
    let (app, _state) = router_with_peer(peer, Some(store));
    let res = app.oneshot(delete_tokens("127.0.0.1")).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "DELETE /api/desktop/tokens must reject non-loopback peers"
    );
}

// ----------------------------------------------------------------------------
// Host-header pin
// ----------------------------------------------------------------------------

#[tokio::test]
async fn rejects_attacker_host_header() {
    let peer: SocketAddr = "127.0.0.1:54321".parse().unwrap();
    let (store, _dir) = fresh_unlocked_store();
    let (app, _state) = router_with_peer(peer, Some(store));
    let res = app
        .oneshot(delete_tokens("attacker.example"))
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "DELETE /api/desktop/tokens must reject DNS-rebinding Host"
    );
}

// ----------------------------------------------------------------------------
// Locked-store surfacing
// ----------------------------------------------------------------------------

#[tokio::test]
async fn returns_503_when_store_locked() {
    let peer: SocketAddr = "127.0.0.1:54321".parse().unwrap();
    let (store, _dir) = fresh_locked_store();
    let (app, _state) = router_with_peer(peer, Some(store));
    let res = app.oneshot(delete_tokens("127.0.0.1")).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "locked store on DELETE must surface as 503"
    );
}

// ----------------------------------------------------------------------------
// Idempotency — DELETE on empty store is 204, not 404
// ----------------------------------------------------------------------------

#[tokio::test]
async fn returns_204_when_no_tokens_persisted() {
    let peer: SocketAddr = "127.0.0.1:54321".parse().unwrap();
    let (store, _dir) = fresh_unlocked_store();
    let (app, _state) = router_with_peer(peer, Some(store));
    let res = app.oneshot(delete_tokens("127.0.0.1")).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::NO_CONTENT,
        "DELETE on absent keys must be idempotent (204, not 404)"
    );
}

// ----------------------------------------------------------------------------
// Happy path — both tokens wiped
// ----------------------------------------------------------------------------

#[tokio::test]
async fn wipes_access_and_refresh_tokens() {
    let peer: SocketAddr = "127.0.0.1:54321".parse().unwrap();
    let (store, _dir) = fresh_unlocked_store();
    store
        .set("entra_access_token", "at", SecretCategory::System)
        .expect("seed access token");
    store
        .set("entra_refresh_token", "rt", SecretCategory::System)
        .expect("seed refresh token");
    let (app, _state) = router_with_peer(peer, Some(store.clone()));
    let res = app.oneshot(delete_tokens("127.0.0.1")).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    assert!(
        store.get("entra_access_token").is_err(),
        "access token must be gone after DELETE"
    );
    assert!(
        store.get("entra_refresh_token").is_err(),
        "refresh token must be gone after DELETE"
    );
}
