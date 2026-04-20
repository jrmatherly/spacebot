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

    let config = spacebot::config::Config::load_from_path(temp.path()).expect("load config");

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

    let config = spacebot::config::Config::load_from_path(temp.path()).expect("load config");

    let provider = config
        .llm
        .providers
        .get("anthropic")
        .expect("anthropic provider populated from top-level [[providers]] array");
    assert_eq!(provider.base_url, mock_server.uri());
    assert_eq!(provider.api_key, "test-key");
}

#[tokio::test]
async fn from_openai_body_parses_litellm_anthropic_cache_tokens() {
    // LiteLLM forwards Anthropic's response through its OpenAI-compat shim.
    // The usage object's exact layout varies by LiteLLM version. This test
    // pins our parser's behavior against the OpenAI-shape nested position
    // (`prompt_tokens_details.cached_tokens`) that LiteLLM emits by default.
    //
    // Research-doc v3 item #9 claimed this parsing was missing. It is not —
    // `src/llm/usage.rs:44-46` already reads the nested position. This test
    // is a regression guard so future refactors don't silently drop it.

    let litellm_response = serde_json::json!({
        "usage": {
            "prompt_tokens": 100,
            "completion_tokens": 50,
            "prompt_tokens_details": {
                "cached_tokens": 80
            }
        }
    });

    let usage = spacebot::llm::usage::ExtendedUsage::from_openai_body(&litellm_response);
    assert_eq!(
        usage.cache_read_tokens, 80,
        "cache_read_tokens must be parsed from prompt_tokens_details.cached_tokens"
    );
    assert_eq!(usage.output_tokens, 50);
    // Non-cached input = 100 - 80 = 20.
    assert_eq!(usage.input_tokens, 20);
}
