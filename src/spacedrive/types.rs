//! Wire types for Spacedrive RPC calls.
//!
//! Mirrors the shape of `spacedrive/crates/sd-client/src/types.rs` but
//! Spacebot-owned. Deliberately minimal — only the fields Spacebot actually
//! reads. Every `Option<T>` field uses `#[serde(default)]` so we survive
//! upstream additions gracefully.
//!
//! Source anchor: `spacedrive/crates/sd-client/src/client.rs:50-55` documents
//! the `{"Query": ...}` / `{"Action": ...}` envelope.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Outer RPC envelope expected by Spacedrive's `/rpc` handler.
/// Spacedrive's daemon expects either `{"Query": QueryRequest}` or
/// `{"Action": QueryRequest}`. The wire-method prefix at the call site decides.
#[derive(Debug, Serialize)]
pub enum RpcEnvelope {
    Query(QueryRequest),
    Action(QueryRequest),
}

/// Payload inside the envelope.
#[derive(Debug, Serialize)]
pub struct QueryRequest {
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library_id: Option<Uuid>,
    pub payload: serde_json::Value,
}

/// Generic RPC response. Spacedrive returns JSON that the caller interprets.
#[derive(Debug, Deserialize)]
pub struct RpcResponse<T> {
    #[serde(default = "Option::default")]
    pub data: Option<T>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Response from `GET /health`. Spacedrive returns a plain `OK` body today;
/// kept as a typed wrapper for future shape expansion.
#[derive(Debug, Deserialize)]
pub struct HealthResponse {
    #[serde(default)]
    pub ok: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_serializes_as_query() {
        let env = RpcEnvelope::Query(QueryRequest {
            method: "query:media_listing".into(),
            library_id: None,
            payload: serde_json::json!({"path": "/"}),
        });
        let out = serde_json::to_value(&env).unwrap();
        assert!(out.get("Query").is_some(), "expected Query key, got {out:?}");
        assert!(out.get("Action").is_none());
    }

    #[test]
    fn envelope_serializes_as_action() {
        let env = RpcEnvelope::Action(QueryRequest {
            method: "action:trigger_scan".into(),
            library_id: None,
            payload: serde_json::Value::Null,
        });
        let out = serde_json::to_value(&env).unwrap();
        assert!(out.get("Action").is_some());
        assert!(out.get("Query").is_none());
    }

    #[test]
    fn response_allows_missing_fields() {
        let src = r#"{}"#;
        let resp: RpcResponse<serde_json::Value> = serde_json::from_str(src).unwrap();
        assert!(resp.data.is_none());
        assert!(resp.error.is_none());
    }
}
