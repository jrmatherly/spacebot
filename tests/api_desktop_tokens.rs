//! Integration tests for `POST /api/desktop/tokens` — the loopback-only
//! token-ingestion endpoint used by the Tauri desktop sign-in flow.
//!
//! The endpoint bypasses the auth middleware (see
//! `tests/api_auth_middleware.rs::desktop_tokens_bypasses_token_check` and
//! its Entra counterpart), so the compensating defenses all live in the
//! handler at `src/api/desktop.rs`. This file locks those defenses so a
//! future refactor that relaxes any of them fails CI:
//!
//!   * Peer IP must satisfy `is_loopback()`.
//!   * `Host` header must match `127.0.0.1`, `[::1]`, or `localhost`.
//!   * Locked `SecretsStore` surfaces as 503 so the Tauri side can prompt
//!     the user to unlock and retry.
//!   * Happy path returns 204 and persists the tokens via `SecretsStore`.

use axum::Router;
use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt as _;
use spacebot::api::ApiState;
use spacebot::api::test_support::build_test_router;
use spacebot::secrets::store::SecretsStore;
use std::net::SocketAddr;
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt as _;

/// Build a test router with a fixed `ConnectInfo<SocketAddr>` layer and an
/// optional secrets store. When `secrets` is `Some`, it is wired into the
/// `ApiState` so `store_desktop_tokens` can reach it.
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

fn post_tokens(path: &str, host: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header(header::HOST, host)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// Fresh unlocked `SecretsStore` on a temp path. Returns the store plus
/// the TempDir so the caller can keep it alive for the test lifetime.
fn fresh_unlocked_store() -> (Arc<SecretsStore>, TempDir) {
    let dir = TempDir::new().expect("temp dir");
    let store = SecretsStore::new(dir.path().join("secrets.redb")).expect("open store");
    (Arc::new(store), dir)
}

/// Fresh locked `SecretsStore`: enable encryption, then lock. Writes will
/// fail with `SecretsError::StoreLocked` until `unlock()` is called.
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
    // Simulate an off-host attacker reaching the allowlisted endpoint.
    let peer: SocketAddr = "10.0.0.1:12345".parse().unwrap();
    let (store, _dir) = fresh_unlocked_store();
    let (app, _state) = router_with_peer(peer, Some(store));
    let res = app
        .oneshot(post_tokens(
            "/api/desktop/tokens",
            "127.0.0.1",
            serde_json::json!({
                "access_token": "at",
                "refresh_token": "rt",
                "expires_in": 3600,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "non-loopback peer must be rejected with 403"
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
    let res = app
        .oneshot(post_tokens(
            "/api/desktop/tokens",
            "attacker.example",
            serde_json::json!({
                "access_token": "at",
                "refresh_token": null,
                "expires_in": 3600,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "DNS-rebinding attack Host must be rejected with 403"
    );
}

#[tokio::test]
async fn accepts_all_three_loopback_host_names() {
    // Each Host name must independently pass. The port suffix is
    // stripped by the handler's split(':') — exercise it too.
    for host in ["127.0.0.1", "127.0.0.1:19898", "[::1]", "localhost"] {
        let peer: SocketAddr = "127.0.0.1:54321".parse().unwrap();
        let (store, _dir) = fresh_unlocked_store();
        let (app, _state) = router_with_peer(peer, Some(store));
        let res = app
            .oneshot(post_tokens(
                "/api/desktop/tokens",
                host,
                serde_json::json!({
                    "access_token": "at",
                    "refresh_token": null,
                    "expires_in": 3600,
                }),
            ))
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            StatusCode::NO_CONTENT,
            "Host {host} must pass the loopback pin"
        );
    }
}

// ----------------------------------------------------------------------------
// Locked-store surfacing (three-layer defense, layer 3)
// ----------------------------------------------------------------------------

#[tokio::test]
async fn returns_503_when_store_locked() {
    let peer: SocketAddr = "127.0.0.1:54321".parse().unwrap();
    let (store, _dir) = fresh_locked_store();
    let (app, _state) = router_with_peer(peer, Some(store));
    let res = app
        .oneshot(post_tokens(
            "/api/desktop/tokens",
            "127.0.0.1",
            serde_json::json!({
                "access_token": "at",
                "refresh_token": "rt",
                "expires_in": 3600,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "locked store must surface as 503 so Tauri shows the unlock prompt"
    );
}

#[tokio::test]
async fn returns_500_when_store_absent() {
    // ApiState without a secrets_store set — operational bug, returns 500.
    let peer: SocketAddr = "127.0.0.1:54321".parse().unwrap();
    let (app, _state) = router_with_peer(peer, None);
    let res = app
        .oneshot(post_tokens(
            "/api/desktop/tokens",
            "127.0.0.1",
            serde_json::json!({
                "access_token": "at",
                "refresh_token": null,
                "expires_in": 3600,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

// ----------------------------------------------------------------------------
// Happy path — access + refresh token both persisted
// ----------------------------------------------------------------------------

#[tokio::test]
async fn persists_access_and_refresh_tokens() {
    let peer: SocketAddr = "127.0.0.1:54321".parse().unwrap();
    let (store, _dir) = fresh_unlocked_store();
    let (app, _state) = router_with_peer(peer, Some(store.clone()));
    let res = app
        .oneshot(post_tokens(
            "/api/desktop/tokens",
            "127.0.0.1",
            serde_json::json!({
                "access_token": "my-access",
                "refresh_token": "my-refresh",
                "expires_in": 3600,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let at = store
        .get("entra_access_token")
        .expect("access token was persisted");
    assert_eq!(at.expose(), "my-access");
    let rt = store
        .get("entra_refresh_token")
        .expect("refresh token was persisted");
    assert_eq!(rt.expose(), "my-refresh");
}

#[tokio::test]
async fn accepts_missing_refresh_token() {
    let peer: SocketAddr = "127.0.0.1:54321".parse().unwrap();
    let (store, _dir) = fresh_unlocked_store();
    let (app, _state) = router_with_peer(peer, Some(store.clone()));
    let res = app
        .oneshot(post_tokens(
            "/api/desktop/tokens",
            "127.0.0.1",
            serde_json::json!({
                "access_token": "my-access",
                "refresh_token": null,
                "expires_in": 3600,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    assert!(
        store.get("entra_access_token").is_ok(),
        "access token persisted"
    );
    assert!(
        store.get("entra_refresh_token").is_err(),
        "refresh token must not be written when absent from body"
    );
    // Drain response body for sanity (204 is empty).
    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert!(body.is_empty());
}
