//! Integration test for `[llm.providers.<id>]` base_url runtime routing.
//!
//! Verifies that when an operator configures
//!
//!     [llm.providers.anthropic]
//!     base_url = "http://localhost:<port>"
//!     api_key = "test"
//!
//! the loaded `ProviderConfig` surfaces that URL, proving the TOML form
//! reaches `LlmConfig.providers` correctly. End-to-end HTTP call coverage
//! (where the LLM dispatch actually hits the wiremock) is deferred to a
//! follow-up agent-level smoke test because building a Rig agent + model
//! in this test harness adds significant scaffolding.

use wiremock::MockServer;

#[tokio::test]
async fn provider_base_url_overrides_default_for_anthropic_table_form() {
    // Wiremock just gives us a stable, reachable URL. We don't actually
    // make a request in this test — see module doc.
    let mock_server = MockServer::start().await;

    let toml = format!(
        r#"
[llm.providers.anthropic]
api_type = "anthropic"
base_url = "{}"
api_key = "test-key"
"#,
        mock_server.uri()
    );

    let temp = tempfile::NamedTempFile::new().expect("temp");
    std::fs::write(temp.path(), &toml).expect("write");

    let config =
        spacebot::config::Config::load_from_path(temp.path()).expect("load config");

    let provider = config
        .llm
        .providers
        .get("anthropic")
        .expect("anthropic provider populated from TOML table form");
    assert_eq!(provider.base_url, mock_server.uri());
    assert_eq!(provider.api_key, "test-key");
}

#[tokio::test]
async fn provider_base_url_overrides_default_for_anthropic_array_form() {
    let mock_server = MockServer::start().await;

    let toml = format!(
        r#"
[[providers]]
name = "anthropic"
api_type = "anthropic"
base_url = "{}"
api_key = "test-key"
"#,
        mock_server.uri()
    );

    let temp = tempfile::NamedTempFile::new().expect("temp");
    std::fs::write(temp.path(), &toml).expect("write");

    let config =
        spacebot::config::Config::load_from_path(temp.path()).expect("load config");

    let provider = config
        .llm
        .providers
        .get("anthropic")
        .expect("anthropic provider populated from top-level [[providers]] array");
    assert_eq!(provider.base_url, mock_server.uri());
    assert_eq!(provider.api_key, "test-key");
}
