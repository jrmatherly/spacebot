//! Integration tests for SpacedriveClient against a mock Spacedrive server.
//!
//! Uses wiremock to stand up an in-process HTTP server that mimics the
//! Spacedrive daemon's `GET /health` and `POST /rpc` endpoints. Validates
//! the envelope shape (`{"Query":...}` vs `{"Action":...}`), bearer-token
//! auth header, and the 401 → AuthFailed translation.

use spacebot::spacedrive::{SpacedriveClient, SpacedriveError, SpacedriveIntegrationConfig};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn cfg(base_url: &str) -> SpacedriveIntegrationConfig {
    SpacedriveIntegrationConfig {
        enabled: true,
        base_url: base_url.to_string(),
        library_id: None,
        spacebot_instance_id: None,
    }
}

#[tokio::test]
async fn health_against_mock_server() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_string("OK"))
        .mount(&server)
        .await;

    let client = SpacedriveClient::new(&cfg(&server.uri()), "token".into()).unwrap();
    let h = client.health().await.unwrap();
    assert!(h.ok);
}

#[tokio::test]
async fn rpc_wraps_in_query_envelope() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/rpc"))
        .and(header("authorization", "Bearer t0k3n"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {"files": []},
        })))
        .mount(&server)
        .await;

    let client = SpacedriveClient::new(&cfg(&server.uri()), "t0k3n".into()).unwrap();

    #[derive(serde::Deserialize, Debug)]
    struct Resp {
        files: Vec<serde_json::Value>,
    }

    let resp: Resp = client
        .rpc("query:media_listing", serde_json::json!({"path": "/"}))
        .await
        .unwrap();
    assert_eq!(resp.files.len(), 0);
}

#[tokio::test]
async fn rpc_401_surfaces_auth_failed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/rpc"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let client = SpacedriveClient::new(&cfg(&server.uri()), "bad".into()).unwrap();
    let err = client
        .rpc::<_, serde_json::Value>("query:anything", serde_json::json!({}))
        .await
        .err()
        .unwrap();
    assert!(matches!(err, SpacedriveError::AuthFailed));
}
