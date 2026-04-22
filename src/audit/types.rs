//! Audit event types. `canonical_bytes` defines the exact serialization
//! used for hashing; any change here invalidates the historical chain, so
//! treat this file as append-only (add variants, never rename).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    AuthSuccess,
    AuthFailure,
    ResourceCreate,
    ResourceRead,
    ResourceWrite,
    ResourceDelete,
    AdminRead,
    AdminWrite,
    AdminClaimResource,
    AuthzDenied,
    OrphanDetected,
    ExportRun,
}

impl AuditAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditAction::AuthSuccess => "auth_success",
            AuditAction::AuthFailure => "auth_failure",
            AuditAction::ResourceCreate => "resource_create",
            AuditAction::ResourceRead => "resource_read",
            AuditAction::ResourceWrite => "resource_write",
            AuditAction::ResourceDelete => "resource_delete",
            AuditAction::AdminRead => "admin_read",
            AuditAction::AdminWrite => "admin_write",
            AuditAction::AdminClaimResource => "admin_claim_resource",
            AuditAction::AuthzDenied => "authz_denied",
            AuditAction::OrphanDetected => "orphan_detected",
            AuditAction::ExportRun => "export_run",
        }
    }
}

/// Event before it's persisted. Missing: `id`, `seq`, `timestamp`,
/// `prev_hash`, `row_hash` — the appender fills these.
#[derive(Debug, Clone)]
pub struct AuditEvent {
    pub principal_key: String,
    pub principal_type: String,
    pub action: AuditAction,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub result: String,
    pub source_ip: Option<String>,
    pub request_id: Option<String>,
    pub metadata: serde_json::Value,
}

/// Persisted row shape.
#[derive(Debug, Clone, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct AuditRow {
    pub id: String,
    pub seq: i64,
    pub timestamp: String,
    pub principal_key: String,
    pub principal_type: String,
    pub action: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub result: String,
    pub source_ip: Option<String>,
    pub request_id: Option<String>,
    pub metadata_json: String,
    pub prev_hash: String,
    pub row_hash: String,
}

/// Canonical bytes for hashing. Concatenates:
///   seq.to_string() || '\n' ||
///   timestamp || '\n' ||
///   principal_key || '\n' ||
///   principal_type || '\n' ||
///   action || '\n' ||
///   resource_type.unwrap_or("") || '\n' ||
///   resource_id.unwrap_or("") || '\n' ||
///   result || '\n' ||
///   source_ip.unwrap_or("") || '\n' ||
///   request_id.unwrap_or("") || '\n' ||
///   canonical_json(metadata) || '\n' ||
///   prev_hash
///
/// Newlines make human inspection possible without an off-by-one delimiter
/// hazard. JSON canonicalization is strict: keys sorted, no whitespace.
pub fn canonical_bytes(event: &AuditEvent, seq: i64, timestamp: &str, prev_hash: &str) -> Vec<u8> {
    let meta_canonical = canonicalize_json(&event.metadata);
    let parts = [
        seq.to_string(),
        timestamp.to_string(),
        event.principal_key.clone(),
        event.principal_type.clone(),
        event.action.as_str().to_string(),
        event.resource_type.clone().unwrap_or_default(),
        event.resource_id.clone().unwrap_or_default(),
        event.result.clone(),
        event.source_ip.clone().unwrap_or_default(),
        event.request_id.clone().unwrap_or_default(),
        meta_canonical,
        prev_hash.to_string(),
    ];
    parts.join("\n").into_bytes()
}

/// JSON canonicalization: recursively sort object keys, emit with no
/// whitespace. Deterministic across runs + platforms.
fn canonicalize_json(v: &serde_json::Value) -> String {
    use serde_json::Value;
    match v {
        Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let body: Vec<String> = entries
                .into_iter()
                .map(|(k, v)| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(k).unwrap(),
                        canonicalize_json(v)
                    )
                })
                .collect();
            format!("{{{}}}", body.join(","))
        }
        Value::Array(arr) => {
            let body: Vec<String> = arr.iter().map(canonicalize_json).collect();
            format!("[{}]", body.join(","))
        }
        _ => serde_json::to_string(v).unwrap(),
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex::encode(digest)
}
