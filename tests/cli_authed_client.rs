//! Integration tests for `AuthedClient`'s 401-refresh path. The mock
//! server stands in for both the daemon's protected endpoint and Entra's
//! `/token` refresh endpoint, so the production Entra URL is never
//! contacted.

use std::sync::Arc;

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

    let req = client.http().get(format!("{}/api/something", server.uri()));
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
    let store_path = tmp.path().join("cli-tokens.json");
    let store = seeded_store(store_path.clone());
    let client = make_authed_client_for_tests(&server.uri(), store);

    let req = client.http().get(format!("{}/api/something", server.uri()));
    let resp = client.send(req).await.expect("expected 200 after refresh");
    assert_eq!(resp.status(), 200);

    // Phase 9 review I5: refresh-token rotation must persist to disk so a
    // CLI restart picks up the new RT. Reload the store from the tempdir
    // path and assert both tokens were written. If `persist_tokens` ever
    // drops the rotation branch, the refresh chain dies after one rotation
    // and only manifests in production at the original RT's TTL (default
    // 90 days).
    let reloaded = CliTokenStore::load_from(&store_path).expect("reload store");
    assert_eq!(reloaded.access_token.as_deref(), Some("fresh-at"));
    assert_eq!(reloaded.refresh_token.as_deref(), Some("rt-2"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_401s_share_a_single_refresh() {
    // Phase 9 review I4: the `refresh_lock` Mutex on AuthedClient exists
    // specifically so that N concurrent send() calls that all 401 on the
    // same expired token serialize on a single refresh, rather than
    // firing N parallel requests against Entra's /token endpoint and
    // tripping the looping-client invalid_grant cutoff. This test pins
    // that contract: two concurrent send() calls must hit the refresh
    // endpoint exactly once between them.
    let server = MockServer::start().await;

    // Refresh endpoint MUST be hit at most once.
    Mock::given(method("POST"))
        .and(path("/oauth2/v2.0/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "fresh-shared-at",
            "refresh_token": "rt-rotated",
            "expires_in": 3600,
        })))
        .expect(1)
        .mount(&server)
        .await;

    // First two GETs return 401 (one per racing caller, since both arrive
    // with the same stale token), then unbounded 200s.
    Mock::given(method("GET"))
        .and(path("/api/something"))
        .respond_with(ResponseTemplate::new(401))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/something"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().expect("tempdir");
    let store_path = tmp.path().join("cli-tokens.json");
    let store = seeded_store(store_path.clone());
    let client = Arc::new(make_authed_client_for_tests(&server.uri(), store));

    let server_uri = server.uri();
    let c1 = Arc::clone(&client);
    let s1 = server_uri.clone();
    let c2 = Arc::clone(&client);
    let s2 = server_uri.clone();
    let h1 = tokio::spawn(async move {
        let req = c1.http().get(format!("{s1}/api/something"));
        c1.send(req).await
    });
    let h2 = tokio::spawn(async move {
        let req = c2.http().get(format!("{s2}/api/something"));
        c2.send(req).await
    });
    let r1 = h1.await.expect("task 1").expect("send 1");
    let r2 = h2.await.expect("task 2").expect("send 2");
    assert_eq!(r1.status(), 200, "first concurrent caller succeeded");
    assert_eq!(r2.status(), 200, "second concurrent caller succeeded");
    // Verify mocks: drop the server (its Drop impl runs `.expect(1)`
    // assertions). Both callers must have shared a single refresh.
    drop(server);

    // Persisted tokens reflect the single shared refresh.
    let reloaded = CliTokenStore::load_from(&store_path).expect("reload store");
    assert_eq!(reloaded.access_token.as_deref(), Some("fresh-shared-at"));
    assert_eq!(reloaded.refresh_token.as_deref(), Some("rt-rotated"));
}
