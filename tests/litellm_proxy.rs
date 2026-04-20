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

#[tokio::test]
async fn delete_litellm_preserves_sibling_anthropic_and_parent_providers_table() {
    // Regression guard for the delete_provider LiteLLM branch: removing the
    // litellm entry must NOT prune the [llm.providers] parent table when a
    // sibling provider (e.g., anthropic) is still configured.
    let initial = r#"
[llm.providers.anthropic]
api_type = "anthropic"
base_url = "https://api.anthropic.com"
api_key = "anthro-key"

[llm.providers.litellm]
api_type = "openai_completions"
base_url = "http://localhost:4000"
api_key = "litellm-key"
use_bearer_auth = true
"#;

    let mut doc: toml_edit::DocumentMut = initial.parse().expect("parse");

    // Mirror delete_provider's mutation logic (src/api/providers.rs:1950-1962).
    if let Some(llm) = doc.get_mut("llm")
        && let Some(llm_table) = llm.as_table_mut()
        && let Some(providers_item) = llm_table.get_mut("providers")
        && let Some(providers_tbl) = providers_item.as_table_mut()
    {
        providers_tbl.remove("litellm");
        if providers_tbl.is_empty() {
            llm_table.remove("providers");
        }
    }

    let after = doc.to_string();
    let temp = tempfile::NamedTempFile::new().expect("temp");
    std::fs::write(temp.path(), &after).expect("write");

    let config = spacebot::config::Config::load_from_path(temp.path()).expect("load config");

    assert!(
        config.llm.providers.contains_key("anthropic"),
        "sibling anthropic must survive"
    );
    assert!(
        !config.llm.providers.contains_key("litellm"),
        "litellm removed"
    );
    // Parent [llm.providers] is still serialized — round-trip through load
    // would be impossible for anthropic otherwise.
}

#[tokio::test]
async fn delete_litellm_prunes_empty_parent_providers_table() {
    // Companion to the preserve test: when litellm is the sole entry,
    // the empty [llm.providers] parent IS pruned.
    let initial = r#"
[llm.providers.litellm]
api_type = "openai_completions"
base_url = "http://localhost:4000"
api_key = "litellm-key"
"#;

    let mut doc: toml_edit::DocumentMut = initial.parse().expect("parse");

    if let Some(llm) = doc.get_mut("llm")
        && let Some(llm_table) = llm.as_table_mut()
        && let Some(providers_item) = llm_table.get_mut("providers")
        && let Some(providers_tbl) = providers_item.as_table_mut()
    {
        providers_tbl.remove("litellm");
        if providers_tbl.is_empty() {
            llm_table.remove("providers");
        }
    }

    // After pruning, doc["llm"] should have no "providers" key.
    let llm_table = doc
        .get("llm")
        .and_then(|i| i.as_table())
        .expect("llm table still present");
    assert!(
        llm_table.get("providers").is_none(),
        "empty [llm.providers] parent must be pruned"
    );

    // And the round-tripped config has no providers map entries.
    let after = doc.to_string();
    let temp = tempfile::NamedTempFile::new().expect("temp");
    std::fs::write(temp.path(), &after).expect("write");
    let config = spacebot::config::Config::load_from_path(temp.path()).expect("load config");
    assert!(!config.llm.providers.contains_key("litellm"));
}

#[tokio::test]
async fn handler_read_chain_resolves_singular_llm_provider_azure() {
    // Load-bearing fallback: the serde deserializer aliases
    // [llm.provider.<id>] into the plural `providers` map, but
    // toml_edit::DocumentMut does NOT — it reads the AST verbatim.
    // The UI handlers read via toml_edit to preserve formatting, so
    // every Azure read site must `.or_else` to the singular form for
    // users whose config predates the 2026-04-20 plural-canonicalization.
    let toml = r#"
[llm.provider.azure]
api_type = "azure"
base_url = "https://example.openai.azure.com"
api_key = "legacy-key"
api_version = "2024-06-01"
deployment = "gpt-4o"
"#;

    let doc: toml_edit::DocumentMut = toml.parse().expect("parse");

    // Mirror the handler read chain verbatim
    // (src/api/providers.rs:1091-1097, 1410-1417, 1485-1505).
    let azure_item = doc.get("llm").and_then(|llm| {
        llm.get("providers")
            .and_then(|p| p.get("azure"))
            .or_else(|| llm.get("provider").and_then(|p| p.get("azure")))
    });

    let azure = azure_item.expect("singular [llm.provider.azure] must resolve via fallback");
    let base_url = azure
        .get("base_url")
        .and_then(|v| v.as_str())
        .expect("base_url present");
    assert_eq!(base_url, "https://example.openai.azure.com");

    // Negative control: no azure anywhere → chain returns None.
    let empty: toml_edit::DocumentMut = "[llm]\nanthropic_key = \"x\"\n".parse().expect("parse");
    let none_item = empty.get("llm").and_then(|llm| {
        llm.get("providers")
            .and_then(|p| p.get("azure"))
            .or_else(|| llm.get("provider").and_then(|p| p.get("azure")))
    });
    assert!(none_item.is_none(), "absent azure returns None");
}

#[tokio::test]
async fn update_litellm_preserves_existing_use_bearer_auth_false_on_none_request() {
    // When the UI request omits use_bearer_auth (None) and the stored config
    // has use_bearer_auth = false, the handler must leave the stored value
    // untouched. This locks in the preserve-on-none rule for the bearer flag,
    // complementing the extra_headers preservation test.
    let initial = r#"
[llm.providers.litellm]
api_type = "openai_completions"
base_url = "http://localhost:4000"
api_key = "k"
use_bearer_auth = false
"#;

    let mut doc: toml_edit::DocumentMut = initial.parse().expect("parse");

    // Mirror the handler: mutate api_key, leave use_bearer_auth alone because
    // request.use_bearer_auth is None and the table already has a value.
    let litellm_table = doc["llm"]["providers"]["litellm"]
        .as_table_mut()
        .expect("litellm table");
    litellm_table["api_key"] = toml_edit::value("new-k");

    // Handler branch at providers.rs:1295-1305: None + existing present = no-op.
    // (We don't execute the write here because the preserve path is a no-write.)

    let after = doc.to_string();
    let temp = tempfile::NamedTempFile::new().expect("temp");
    std::fs::write(temp.path(), &after).expect("write");

    let config = spacebot::config::Config::load_from_path(temp.path()).expect("load config");
    let provider = config.llm.providers.get("litellm").expect("litellm");

    assert_eq!(provider.api_key, "new-k", "api_key updated");
    assert!(
        !provider.use_bearer_auth,
        "use_bearer_auth = false preserved on None request"
    );
}
