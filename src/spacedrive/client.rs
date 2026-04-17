//! HTTP client for calling a paired Spacedrive instance.
//!
//! Constructs its own `reqwest::Client` with explicit connect/timeout bounds
//! per reviewer sweep Rust-2. Does NOT follow the `src/llm/` pattern; those
//! are Rig-framework adapters, not reqwest clients.
//!
//! Wire format: every call wraps the `QueryRequest` in either
//! `{"Query": ...}` or `{"Action": ...}` per the Spacedrive daemon's
//! expectation (see `spacedrive/crates/sd-client/src/client.rs:50-55`).
//!
//! Graceful degradation: a 401 from Spacedrive triggers a `SpacedriveError::
//! AuthFailed` that callers can use to reload the token from the secrets
//! store. Spacedrive unreachable returns `Http`; callers decide retry shape.

use crate::spacedrive::config::SpacedriveIntegrationConfig;
use crate::spacedrive::error::{Result, SpacedriveError};
use crate::spacedrive::types::{HealthResponse, QueryRequest, RpcEnvelope, RpcResponse};

use reqwest::{Client, StatusCode, Url, header};
use serde::Serialize;
use serde::de::DeserializeOwned;

use std::time::Duration;
use uuid::Uuid;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_RESPONSE_CAP: usize = 10 * 1024 * 1024;

pub struct SpacedriveClient {
    http: Client,
    base_url: Url,
    auth_token: String,
    library_id: Option<Uuid>,
    response_cap: usize,
}

impl SpacedriveClient {
    /// Construct a client from a resolved config + auth token.
    ///
    /// Caller is responsible for loading `auth_token` from
    /// `src/secrets/store.rs` (key `spacedrive_auth_token:<library_id>`) per
    /// pairing ADR D3. This constructor does not touch secrets directly.
    ///
    /// Returns `InsecureBaseUrl` if `base_url` is plain `http://` to a
    /// non-loopback host.
    pub fn new(cfg: &SpacedriveIntegrationConfig, auth_token: String) -> Result<Self> {
        let base_url: Url = cfg.base_url.parse().map_err(|e: url::ParseError| {
            SpacedriveError::Wire(format!("invalid base_url: {e}"))
        })?;

        if base_url.scheme() == "http" {
            let host = base_url.host_str().unwrap_or("");
            let is_loopback = host == "localhost" || host == "127.0.0.1" || host == "::1";
            if !is_loopback {
                return Err(SpacedriveError::InsecureBaseUrl {
                    host: host.to_string(),
                });
            }
        }

        let http = Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
            .build()?;

        Ok(Self {
            http,
            base_url,
            auth_token,
            library_id: cfg.library_id,
            response_cap: DEFAULT_RESPONSE_CAP,
        })
    }

    /// GET /health probe.
    #[tracing::instrument(skip(self))]
    pub async fn health(&self) -> Result<HealthResponse> {
        let url = self
            .base_url
            .join("health")
            .map_err(|e| SpacedriveError::Wire(e.to_string()))?;
        let resp = self.http.get(url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(SpacedriveError::HttpStatus {
                status: status.as_u16(),
            });
        }
        let body = resp.text().await?;
        Ok(HealthResponse {
            ok: !body.is_empty(),
        })
    }

    /// POST /rpc — the primary wire path.
    ///
    /// `wire_method` prefix determines envelope shape: `query:...` → Query,
    /// anything else → Action.
    #[tracing::instrument(skip(self, payload))]
    pub async fn rpc<I: Serialize, O: DeserializeOwned>(
        &self,
        wire_method: &str,
        payload: I,
    ) -> Result<O> {
        let url = self
            .base_url
            .join("rpc")
            .map_err(|e| SpacedriveError::Wire(e.to_string()))?;

        let payload_value = serde_json::to_value(payload)?;
        let query_req = QueryRequest {
            method: wire_method.to_string(),
            library_id: self.library_id,
            payload: payload_value,
        };

        let envelope = if wire_method.starts_with("query:") {
            RpcEnvelope::Query(query_req)
        } else {
            RpcEnvelope::Action(query_req)
        };

        let resp = self
            .http
            .post(url)
            .header(header::AUTHORIZATION, format!("Bearer {}", self.auth_token))
            .json(&envelope)
            .send()
            .await?;

        let status = resp.status();
        if status == StatusCode::UNAUTHORIZED {
            return Err(SpacedriveError::AuthFailed);
        }
        if !status.is_success() {
            return Err(SpacedriveError::HttpStatus {
                status: status.as_u16(),
            });
        }

        let bytes = resp.bytes().await?;
        if bytes.len() > self.response_cap {
            return Err(SpacedriveError::ResponseTooLarge {
                actual: bytes.len(),
                cap: self.response_cap,
            });
        }

        let rpc_resp: RpcResponse<O> = serde_json::from_slice(&bytes)?;
        if let Some(err) = rpc_resp.error {
            return Err(SpacedriveError::Wire(err));
        }
        rpc_resp
            .data
            .ok_or_else(|| SpacedriveError::Wire("empty response data".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_http_to_non_loopback() {
        let cfg = SpacedriveIntegrationConfig {
            enabled: true,
            base_url: "http://example.com".into(),
            library_id: None,
            spacebot_instance_id: None,
        };
        let err = SpacedriveClient::new(&cfg, "token".into()).err().unwrap();
        assert!(matches!(err, SpacedriveError::InsecureBaseUrl { .. }));
    }

    #[test]
    fn accepts_http_to_localhost() {
        let cfg = SpacedriveIntegrationConfig {
            enabled: true,
            base_url: "http://127.0.0.1:8080".into(),
            library_id: None,
            spacebot_instance_id: None,
        };
        assert!(SpacedriveClient::new(&cfg, "token".into()).is_ok());
    }

    #[test]
    fn accepts_https_anywhere() {
        let cfg = SpacedriveIntegrationConfig {
            enabled: true,
            base_url: "https://spacedrive.example.com".into(),
            library_id: None,
            spacebot_instance_id: None,
        };
        assert!(SpacedriveClient::new(&cfg, "token".into()).is_ok());
    }
}
