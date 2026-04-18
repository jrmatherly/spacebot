//! spacedrive_list_files: list files in a paired Spacedrive library.
//!
//! Calls Spacedrive's existing `query:media_listing` RPC. Wraps the response
//! in the prompt-injection envelope per
//! `docs/design-docs/spacedrive-tool-response-envelope.md`.
//!
//! Requires: `[spacedrive] enabled = true` and a populated `library_id`. The
//! caller (`src/tools.rs`) is responsible for passing a constructed client;
//! the tool itself does not read config or secrets.

use crate::spacedrive::envelope::{CAP_LIST_FILES, wrap_spacedrive_response};
use crate::spacedrive::{SpacedriveClient, SpacedriveError};

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use std::sync::Arc;

#[derive(Debug, Clone)]
pub(crate) struct SpacedriveListFilesContext {
    client: Arc<SpacedriveClient>,
    library_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SpacedriveListFilesArgs {
    /// Path within the paired library to list (e.g. "/", "/Documents").
    pub path: String,
    /// Optional max number of entries to return (Spacedrive may still cap).
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
struct MediaListingRequest {
    path: String,
    limit: Option<usize>,
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
                .unwrap(),
        }
    }

    #[tracing::instrument(skip(self, args))]
    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let req = MediaListingRequest {
            path: args.path,
            limit: args.limit,
        };
        let payload: serde_json::Value =
            self.context.client.rpc("query:media_listing", req).await?;
        let raw = serde_json::to_vec(&payload)?;
        Ok(wrap_spacedrive_response(
            &self.context.library_id,
            "query:media_listing",
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
        let src = serde_json::json!({"path": "/", "limit": 10});
        let args: SpacedriveListFilesArgs = serde_json::from_value(src).unwrap();
        assert_eq!(args.path, "/");
        assert_eq!(args.limit, Some(10));
    }

    #[test]
    fn limit_is_optional() {
        let src = serde_json::json!({"path": "/"});
        let args: SpacedriveListFilesArgs = serde_json::from_value(src).unwrap();
        assert_eq!(args.limit, None);
    }

    #[tokio::test]
    async fn list_files_wraps_response_in_envelope() {
        use crate::spacedrive::{SpacedriveClient, SpacedriveIntegrationConfig};
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rpc"))
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
                path: "/".into(),
                limit: None,
            })
            .await
            .unwrap();

        assert!(out.starts_with("[SPACEDRIVE:test-lib:query:media_listing]"));
        assert!(out.contains("<<<UNTRUSTED_SPACEDRIVE_CONTENT>>>"));
        assert!(out.contains("<<<END_UNTRUSTED_SPACEDRIVE_CONTENT>>>"));
        assert!(out.contains("report.pdf"));
    }
}
