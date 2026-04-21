//! Microsoft Graph API client for group resolution and user photo fetch.
//! Handles JWT `groups`-claim overage (SPA 6 / JWT 200 thresholds per
//! research §12 E-2) and the A-19 photo cache refresh.
//!
//! Permission model: delegated `User.Read` via OBO. One scope covers both
//! `/me/getMemberObjects` (Task 3.1 decision) and `/me/photo/$value` (A-19).
//! See `docs/design-docs/entra-app-registrations.md` § "Graph API permissions".

use reqwest::{Client, header};
use serde::Deserialize;

use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("graph http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("graph returned {status}: {body}")]
    Status { status: u16, body: String },
    #[error("OBO token exchange failed: {0}")]
    OboFailed(String),
}

#[derive(Debug, Clone)]
pub struct GraphConfig {
    pub tenant_id: Arc<str>,
    pub web_api_client_id: Arc<str>,
    /// Resolved from the secret store at startup. Never logged.
    pub web_api_client_secret: Arc<str>,
    /// Default `https://graph.microsoft.com/v1.0`. Override only for tests.
    pub graph_api_base: Arc<str>,
    /// Default `https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token`.
    /// Configurable so integration tests can point it at a Wiremock server
    /// alongside the Graph stubs. `main.rs` constructs the production URL.
    pub obo_token_endpoint: Arc<str>,
    pub request_timeout_secs: u64,
}

#[derive(Debug)]
pub struct GraphClient {
    cfg: GraphConfig,
    http: Client,
}

#[derive(Debug, Deserialize)]
struct OboTokenResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
pub struct GraphGroup {
    /// Object GUID. This is the `external_id` in the `teams` table.
    pub id: String,
    #[serde(default, rename = "displayName")]
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GraphListResponse<T> {
    value: Vec<T>,
    #[serde(default, rename = "@odata.nextLink")]
    next_link: Option<String>,
}

impl GraphClient {
    pub fn new(cfg: GraphConfig) -> Result<Self, GraphError> {
        let http = Client::builder()
            .timeout(Duration::from_secs(cfg.request_timeout_secs))
            .build()?;
        Ok(Self { cfg, http })
    }

    /// Exchange a user's access token for a Graph-scoped token (OBO).
    ///
    /// Scope: `User.Read`. Least-privileged delegated permission that covers
    /// both `/me/getMemberObjects` (signed-in-user transitive memberships,
    /// Task 3.1 decision) and `/me/photo/$value` (A-19 photo fetch). A single
    /// OBO exchange serves both operations.
    #[tracing::instrument(skip(self, user_token))]
    async fn obo_exchange(&self, user_token: &str) -> Result<String, GraphError> {
        let url = self.cfg.obo_token_endpoint.as_ref();
        let params = [
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("client_id", self.cfg.web_api_client_id.as_ref()),
            ("client_secret", self.cfg.web_api_client_secret.as_ref()),
            ("assertion", user_token),
            ("scope", "https://graph.microsoft.com/User.Read"),
            ("requested_token_use", "on_behalf_of"),
        ];
        let res = self.http.post(url).form(&params).send().await?;
        let status = res.status();
        if !status.is_success() {
            let body = res.text().await.unwrap_or_default();
            tracing::warn!(
                graph.obo.status = %status,
                "OBO token exchange failed",
            );
            return Err(GraphError::OboFailed(format!("{status} {body}")));
        }
        let token: OboTokenResponse = res.json().await?;
        Ok(token.access_token)
    }

    /// Resolve the signed-in user's transitive group memberships. Used for
    /// overage resolution and periodic reconciliation.
    ///
    /// `POST /me/getMemberObjects` with `securityEnabledOnly: false`. The
    /// `/me/` path requires only `User.Read`; the `/users/{id}/` variant
    /// would require `User.ReadBasic.All + GroupMember.Read.All`.
    ///
    /// Research §12 E-3 supersedes a prior choice of `/memberOf` (direct
    /// memberships only) in favor of `getMemberObjects` (transitive).
    #[tracing::instrument(skip(self, user_access_token))]
    pub async fn list_member_groups(
        &self,
        user_access_token: &str,
    ) -> Result<Vec<GraphGroup>, GraphError> {
        let graph_token = self.obo_exchange(user_access_token).await?;

        // getMemberObjects is an OData action. Actions don't support $top /
        // $skip / @odata.nextLink pagination. Response is a flat string array
        // of IDs in a single body. Pagination only applies to the
        // /groups?$filter lookup below (Amendment A-11).
        let url = format!("{}/me/getMemberObjects", self.cfg.graph_api_base);
        let payload = serde_json::json!({ "securityEnabledOnly": false });
        let res = self
            .http
            .post(&url)
            .bearer_auth(&graph_token)
            .json(&payload)
            .send()
            .await?;
        let status = res.status();
        if !status.is_success() {
            let body = res.text().await.unwrap_or_default();
            tracing::warn!(
                graph.endpoint = "getMemberObjects",
                graph.status = %status,
                "getMemberObjects failed",
            );
            return Err(GraphError::Status {
                status: status.as_u16(),
                body,
            });
        }
        #[derive(Deserialize)]
        struct ObjResponse {
            value: Vec<String>,
        }
        let ids: ObjResponse = res.json().await?;

        // Second pass: fetch display names. Batch in groups of 15 (Graph
        // $filter documented safety limit for disjunction sets).
        // A-11: MUST follow @odata.nextLink to completion per page, otherwise
        // large result sets silently drop groups.
        let mut groups: Vec<GraphGroup> = Vec::with_capacity(ids.value.len());
        for chunk in ids.value.chunks(15) {
            let filter = chunk
                .iter()
                .map(|id| format!("id eq '{id}'"))
                .collect::<Vec<_>>()
                .join(" or ");
            let mut next_url = Some(format!(
                "{}/groups?$select=id,displayName&$filter={}",
                self.cfg.graph_api_base,
                urlencoding::encode(&filter)
            ));
            while let Some(url) = next_url.take() {
                let page: GraphListResponse<GraphGroup> = self
                    .http
                    .get(&url)
                    .bearer_auth(&graph_token)
                    .send()
                    .await?
                    .error_for_status()?
                    .json()
                    .await?;
                groups.extend(page.value);
                next_url = page.next_link;
            }
        }
        Ok(groups)
    }

    /// Fetch the signed-in user's profile photo from Microsoft Graph (A-19).
    ///
    /// Returns `Ok(None)` when the user has no photo set (Graph returns 404).
    /// Returns binary JPEG bytes for caller to base64-encode and persist.
    #[tracing::instrument(skip(self, user_access_token))]
    pub async fn fetch_user_photo(
        &self,
        user_access_token: &str,
    ) -> Result<Option<Vec<u8>>, GraphError> {
        let graph_token = self.obo_exchange(user_access_token).await?;
        let url = format!("{}/me/photo/$value", self.cfg.graph_api_base);
        let res = self
            .http
            .get(&url)
            .bearer_auth(&graph_token)
            .header(header::ACCEPT, "image/jpeg")
            .send()
            .await?;
        match res.status().as_u16() {
            200 => {
                let bytes = res.bytes().await?.to_vec();
                Ok(Some(bytes))
            }
            // No photo on the M365 profile. SPA falls back to initials.
            404 => Ok(None),
            status => {
                let body = res.text().await.unwrap_or_default();
                tracing::warn!(
                    graph.endpoint = "photo",
                    graph.status = %status,
                    "photo fetch failed",
                );
                Err(GraphError::Status { status, body })
            }
        }
    }
}
