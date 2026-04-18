//! spacedrive_list_files: list files in a paired Spacedrive library.
//!
//! Calls Spacedrive's `query:files.media_listing` RPC (see
//! `spacedrive/crates/sd-client/src/client.rs:59-94`). Wraps the response
//! in the prompt-injection envelope per
//! `docs/design-docs/spacedrive-tool-response-envelope.md`.
//!
//! Requires: `[spacedrive] enabled = true` and a populated `library_id`. The
//! caller (`src/tools.rs`) is responsible for passing a constructed client.
//! The tool itself does not read config or secrets.

use crate::spacedrive::envelope::{CAP_LIST_FILES, wrap_spacedrive_response};
use crate::spacedrive::types::SdPath;
use crate::spacedrive::{SpacedriveClient, SpacedriveError};

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use std::sync::Arc;

const WIRE_METHOD: &str = "query:files.media_listing";
const DEFAULT_SORT_BY: &str = "datetaken";

#[derive(Debug, Clone)]
pub(crate) struct SpacedriveListFilesContext {
    client: Arc<SpacedriveClient>,
    library_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SpacedriveListFilesArgs {
    /// Device slug identifying the paired library host (e.g. "jamies-macbook").
    pub device_slug: String,
    /// Filesystem path on that device to list (e.g. "/", "/Documents").
    pub path: String,
    /// Optional max number of entries to return. Spacedrive may still apply
    /// its own cap.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Whether to include descendants of the path. Defaults to true, matching
    /// the Spacedrive client's default.
    #[serde(default = "default_include_descendants")]
    pub include_descendants: bool,
}

fn default_include_descendants() -> bool {
    true
}

/// Input payload for `query:files.media_listing`. Mirrors
/// `MediaListingInput` at `spacedrive/crates/sd-client/src/client.rs:61-67`.
#[derive(Debug, Serialize)]
struct MediaListingInput {
    path: SdPath,
    include_descendants: bool,
    media_types: Option<Vec<String>>,
    limit: Option<usize>,
    sort_by: String,
}

pub(crate) struct SpacedriveListFilesTool {
    context: SpacedriveListFilesContext,
}

impl Tool for SpacedriveListFilesTool {
    const NAME: &'static str = "spacedrive_list_files";
    type Error = SpacedriveError;
    type Args = SpacedriveListFilesArgs;
    type Output = String;

    async fn definition(&self, _: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: crate::prompts::text::get("tools/spacedrive_list_files").to_string(),
            parameters: serde_json::to_value(schemars::schema_for!(SpacedriveListFilesArgs))
                .expect("SpacedriveListFilesArgs schema must serialize"),
        }
    }

    #[tracing::instrument(skip(self, args), fields(library_id = %self.context.library_id, path = %args.path))]
    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let input = MediaListingInput {
            path: SdPath::Physical {
                device_slug: args.device_slug,
                path: args.path,
            },
            include_descendants: args.include_descendants,
            media_types: None,
            limit: args.limit,
            sort_by: DEFAULT_SORT_BY.to_string(),
        };
        let payload: serde_json::Value = self.context.client.rpc(WIRE_METHOD, input).await?;
        let raw = serde_json::to_vec(&payload)?;
        Ok(wrap_spacedrive_response(
            &self.context.library_id,
            WIRE_METHOD,
            &raw,
            CAP_LIST_FILES,
        ))
    }
}

/// Register Spacedrive tools on the given ToolServer. Mirrors the
/// `register_file_tools` / `register_browser_tools` pattern at
/// `src/tools/file.rs:620`.
pub fn register_spacedrive_tools(
    server: rig::tool::server::ToolServer,
    client: Arc<SpacedriveClient>,
    library_id: String,
) -> rig::tool::server::ToolServer {
    let context = SpacedriveListFilesContext { client, library_id };
    server.tool(SpacedriveListFilesTool { context })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_round_trip() {
        let src = serde_json::json!({"device_slug": "dev", "path": "/", "limit": 10});
        let args: SpacedriveListFilesArgs = serde_json::from_value(src).unwrap();
        assert_eq!(args.device_slug, "dev");
        assert_eq!(args.path, "/");
        assert_eq!(args.limit, Some(10));
        assert!(args.include_descendants, "default true when omitted");
    }

    #[test]
    fn limit_is_optional() {
        let src = serde_json::json!({"device_slug": "dev", "path": "/"});
        let args: SpacedriveListFilesArgs = serde_json::from_value(src).unwrap();
        assert_eq!(args.limit, None);
    }

    #[test]
    fn request_payload_serializes_as_sdpath_physical() {
        // Pin the wire shape against the upstream MediaListingInput at
        // spacedrive/crates/sd-client/src/client.rs:61-67. Any drift from
        // externally-tagged SdPath or the five-field input struct breaks this.
        let input = MediaListingInput {
            path: SdPath::Physical {
                device_slug: "jamies-macbook".into(),
                path: "/Documents".into(),
            },
            include_descendants: true,
            media_types: None,
            limit: Some(25),
            sort_by: DEFAULT_SORT_BY.into(),
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(
            json["path"]["Physical"]["device_slug"],
            serde_json::json!("jamies-macbook")
        );
        assert_eq!(
            json["path"]["Physical"]["path"],
            serde_json::json!("/Documents")
        );
        assert_eq!(json["include_descendants"], serde_json::json!(true));
        assert_eq!(json["sort_by"], serde_json::json!("datetaken"));
        assert_eq!(json["limit"], serde_json::json!(25));
        assert!(json["media_types"].is_null());
    }

    #[test]
    fn description_key_is_registered() {
        // Regression guard: the tool's description reads from prompts::text;
        // if the key is unregistered, get() returns "" and logs an error on
        // every definition() call. Pin both conditions here.
        let desc = crate::prompts::text::get("tools/spacedrive_list_files");
        assert!(
            !desc.is_empty(),
            "prompts/en/tools/spacedrive_list_files not registered in src/prompts/text.rs"
        );
        assert!(
            desc.contains("UNTRUSTED"),
            "description should surface the injection-defense fence to the LLM"
        );
    }

    #[tokio::test]
    async fn list_files_wraps_response_in_envelope() {
        use crate::spacedrive::{SpacedriveClient, SpacedriveIntegrationConfig};
        use wiremock::matchers::{body_partial_json, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        // Assert the wire envelope contains the Query tag and the SdPath::Physical
        // shape. This guards against drift in both the client's envelope logic
        // and the tool's payload construction.
        Mock::given(method("POST"))
            .and(path("/rpc"))
            .and(body_partial_json(serde_json::json!({
                "Query": {
                    "method": "query:files.media_listing",
                    "payload": {
                        "path": {"Physical": {"device_slug": "dev", "path": "/"}},
                    }
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {"files": [{"name": "report.pdf"}]},
            })))
            .mount(&server)
            .await;

        let cfg = SpacedriveIntegrationConfig {
            enabled: true,
            base_url: server.uri(),
            library_id: None,
            spacebot_instance_id: None,
        };
        let client = SpacedriveClient::new(&cfg, "token".into()).unwrap();
        let tool = SpacedriveListFilesTool {
            context: SpacedriveListFilesContext {
                client: Arc::new(client),
                library_id: "test-lib".into(),
            },
        };

        use rig::tool::Tool as _;
        let out = tool
            .call(SpacedriveListFilesArgs {
                device_slug: "dev".into(),
                path: "/".into(),
                limit: None,
                include_descendants: true,
            })
            .await
            .unwrap();

        assert!(out.starts_with("[SPACEDRIVE:test-lib:query:files.media_listing]"));
        assert!(out.contains("<<<UNTRUSTED_SPACEDRIVE_CONTENT>>>"));
        assert!(out.contains("<<<END_UNTRUSTED_SPACEDRIVE_CONTENT>>>"));
        assert!(out.contains("report.pdf"));
    }
}
