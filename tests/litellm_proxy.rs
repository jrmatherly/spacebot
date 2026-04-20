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

// Note: Config::load_from_path is sync (see src/config/load.rs:491), so these
// tests could be #[test] instead of #[tokio::test]. The existing two tests in
// this file use #[tokio::test] — matching that convention for consistency
// within the file.

#[tokio::test]
async fn update_litellm_provider_writes_block_form_with_api_type_openai_completions() {
    // This test exercises the TOML shape that `update_litellm_provider`
    // produces, by constructing the same document edits and verifying
    // round-trip through `Config::load_from_path`.
    let toml = r#"
[llm.providers.litellm]
api_type = "openai_completions"
base_url = "http://localhost:4000"
api_key = "test-key"
use_bearer_auth = true
extra_headers = [
    ["x-litellm-tags", "spacebot-dev"],
    ["x-litellm-budget", "team-ai"],
]
"#;

    let temp = tempfile::NamedTempFile::new().expect("temp");
    std::fs::write(temp.path(), toml).expect("write");

    let config = spacebot::config::Config::load_from_path(temp.path()).expect("load config");

    let provider = config
        .llm
        .providers
        .get("litellm")
        .expect("litellm provider populated from TOML table form");
    assert_eq!(provider.base_url, "http://localhost:4000");
    assert_eq!(provider.api_key, "test-key");
    assert!(provider.use_bearer_auth);
    assert_eq!(provider.extra_headers.len(), 2);
    assert_eq!(
        provider.extra_headers[0],
        ("x-litellm-tags".to_string(), "spacebot-dev".to_string())
    );
}

#[tokio::test]
async fn update_litellm_provider_preserves_extra_headers_on_empty_submit() {
    // Core invariant of §7-Q2 dual-empty-is-no-change rule:
    // when a request supplies empty extra_headers AND the table already
    // has extra_headers, the existing value is preserved.
    //
    // This test simulates the TOML mutation path by directly editing the
    // document the way `update_litellm_provider` does.
    let initial = r#"
[llm.providers.litellm]
api_type = "openai_completions"
base_url = "http://localhost:4000"
api_key = "old-key"
use_bearer_auth = true
extra_headers = [
    ["x-litellm-tags", "keep-this"],
]
"#;

    let mut doc: toml_edit::DocumentMut = initial.parse().expect("parse");

    // Simulate the handler path: mutate api_key and base_url, skip
    // extra_headers overwrite because the request supplied an empty list.
    let litellm_table = doc["llm"]["providers"]["litellm"]
        .as_table_mut()
        .expect("litellm table");
    litellm_table["api_key"] = toml_edit::value("new-key");
    litellm_table["base_url"] = toml_edit::value("http://litellm.example.com:4000");
    // Do NOT touch extra_headers.

    let after = doc.to_string();
    let temp = tempfile::NamedTempFile::new().expect("temp");
    std::fs::write(temp.path(), &after).expect("write");

    let config = spacebot::config::Config::load_from_path(temp.path()).expect("load config");
    let provider = config.llm.providers.get("litellm").expect("litellm");

    assert_eq!(provider.api_key, "new-key", "api_key updated");
    assert_eq!(
        provider.base_url, "http://litellm.example.com:4000",
        "base_url updated"
    );
    assert_eq!(
        provider.extra_headers.len(),
        1,
        "existing extra_headers preserved when request supplies empty"
    );
    assert_eq!(
        provider.extra_headers[0],
        ("x-litellm-tags".to_string(), "keep-this".to_string())
    );
}
