//! Integration tests for `AuthedClient`'s 401-refresh path. The mock
//! server stands in for both the daemon's protected endpoint and Entra's
//! `/token` refresh endpoint, so the production Entra URL is never
//! contacted.

use spacebot::cli::http::AuthedClient;
use spacebot::cli::store::CliTokenStore;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn seeded_store(path: std::path::PathBuf) -> CliTokenStore {
    let mut store = CliTokenStore::with_path(path);
    store.access_token = Some("stale-at".into());
    store.refresh_token = Some("rt-1".into());
    store
}

fn make_authed_client_for_tests(server_uri: &str, store: CliTokenStore) -> AuthedClient {
    let token_url = format!("{server_uri}/oauth2/v2.0/token");
    AuthedClient::with_token_url(
        store,
        server_uri.to_string(),
        "test-client".to_string(),
        token_url,
    )
}

#[tokio::test]
async fn send_returns_error_when_401_persists_after_refresh() {
    let server = MockServer::start().await;

    // Refresh endpoint returns a fresh token...
    Mock::given(method("POST"))
        .and(path("/oauth2/v2.0/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "fresh-still-bad",
            "expires_in": 3600,
        })))
        .mount(&server)
        .await;

    // ...but the protected endpoint returns 401 every time, simulating
    // a fundamentally invalid token (e.g. revoked refresh chain).
    Mock::given(method("GET"))
        .and(path("/api/something"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().expect("tempdir");
    let store = seeded_store(tmp.path().join("cli-tokens.json"));
    let client = make_authed_client_for_tests(&server.uri(), store);

    let req = client
        .http()
        .get(format!("{}/api/something", server.uri()));
    let err = client
        .send(req)
        .await
        .expect_err("expected bail on persistent 401");
    assert!(
        err.to_string().contains("401"),
        "unexpected error message: {err}"
    );
}

#[tokio::test]
async fn send_succeeds_after_one_refresh() {
    let server = MockServer::start().await;

    // Refresh endpoint always returns a fresh access_token.
    Mock::given(method("POST"))
        .and(path("/oauth2/v2.0/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "fresh-at",
            "refresh_token": "rt-2",
            "expires_in": 3600,
        })))
        .mount(&server)
        .await;

    // First GET → 401, second GET → 200. wiremock dispatches in mount
    // order: the `up_to_n_times(1)` mock answers the first call, then
    // the unbounded mock takes over.
    Mock::given(method("GET"))
        .and(path("/api/something"))
        .respond_with(ResponseTemplate::new(401))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/something"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().expect("tempdir");
    let store = seeded_store(tmp.path().join("cli-tokens.json"));
    let client = make_authed_client_for_tests(&server.uri(), store);

    let req = client
        .http()
        .get(format!("{}/api/something", server.uri()));
    let resp = client.send(req).await.expect("expected 200 after refresh");
    assert_eq!(resp.status(), 200);
}
