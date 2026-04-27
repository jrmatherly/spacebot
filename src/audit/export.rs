//! Daily WORM export. Filesystem mode is dev-only and NOT tamper-evident (per A-15, which
//! removed chattr +i because it silently fails in non-root containers). Production deployments MUST use
//! S3 Object Lock (`ExportMode::S3`) or append-only SIEM ingestion (`ExportMode::HttpSiem`); both
//! are fully-serializable enum variants whose implementations land in Phase 10 SOC 2 hardening.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::audit::types::AuditRow;
use crate::db::DbPool;

/// Postgres timestamp-column projection for `audit_events`. Casts
/// `timestamp` (TIMESTAMPTZ) through `to_char(... AT TIME ZONE 'UTC', ...)`
/// so the all-`String` `AuditRow` `FromRow` derive works uniformly across
/// both backends. SQLite arms keep `SELECT *` because their TEXT columns
/// already deserialize as `String`.
const PG_AUDIT_EVENTS_COLUMNS: &str = "seq, id, \
    to_char(\"timestamp\" AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"') AS \"timestamp\", \
    principal_key, principal_type, action, resource_type, resource_id, \
    result, source_ip, request_id, metadata_json, prev_hash, row_hash";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExportMode {
    /// Local filesystem export. DEV ONLY. Not claimed as tamper-evident.
    /// Production deployments MUST use `S3` or `HttpSiem`.
    Filesystem { dir: PathBuf },
    /// A-15: S3-compatible object storage with Object Lock. Works with AWS
    /// S3, MinIO, Cloudflare R2, OCI Object Storage.
    S3 {
        endpoint: String,
        bucket: String,
        prefix: String,
        region: String,
        access_key_id: String,
        secret_access_key: String,
        retention_days: u32,
        /// "GOVERNANCE" | "COMPLIANCE" — match S3 Object Lock semantics.
        object_lock_mode: String,
    },
    /// A-15: ship events to an external SIEM via HTTPS POST. SIEM owns
    /// retention; daemon is stateless w.r.t. long-term retention.
    HttpSiem {
        endpoint: String,
        auth_header: String,
        batch_size: usize,
    },
}

impl ExportMode {
    /// Stable string for the `audit_export_state.export_mode` column.
    pub fn kind_str(&self) -> &'static str {
        match self {
            ExportMode::Filesystem { .. } => "filesystem",
            ExportMode::S3 { .. } => "s3",
            ExportMode::HttpSiem { .. } => "http_siem",
        }
    }

    pub fn is_tamper_evident(&self) -> bool {
        matches!(self, ExportMode::S3 { .. } | ExportMode::HttpSiem { .. })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExportConfig {
    pub enabled: bool,
    pub mode: ExportMode,
}

#[derive(Debug)]
pub struct ExportResult {
    pub rows_exported: usize,
    pub first_seq: Option<i64>,
    pub last_seq: Option<i64>,
    pub chain_head_hash: Option<String>,
}

pub async fn export_audit(pool: &DbPool, cfg: &ExportConfig) -> anyhow::Result<ExportResult> {
    if !cfg.enabled {
        return Ok(ExportResult {
            rows_exported: 0,
            first_seq: None,
            last_seq: None,
            chain_head_hash: None,
        });
    }

    // A-14: incremental export. Read `last_exported_seq` for this export
    // mode; fetch only newer rows.
    let mode_kind = cfg.mode.kind_str();
    let last_exported_seq: i64 = match pool {
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT last_exported_seq FROM audit_export_state WHERE export_mode = ?",
        )
        .bind(mode_kind)
        .fetch_optional(p)
        .await?
        .unwrap_or(0),
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT last_exported_seq FROM audit_export_state WHERE export_mode = $1",
        )
        .bind(mode_kind)
        .fetch_optional(p)
        .await?
        .unwrap_or(0),
    };

    let rows: Vec<AuditRow> = match pool {
        DbPool::Sqlite(p) => {
            sqlx::query_as("SELECT * FROM audit_events WHERE seq > ? ORDER BY seq")
                .bind(last_exported_seq)
                .fetch_all(p)
                .await?
        }
        DbPool::Postgres(p) => sqlx::query_as(&format!(
            "SELECT {PG_AUDIT_EVENTS_COLUMNS} FROM audit_events WHERE seq > $1 ORDER BY seq"
        ))
        .bind(last_exported_seq)
        .fetch_all(p)
        .await?,
    };
    if rows.is_empty() {
        return Ok(ExportResult {
            rows_exported: 0,
            first_seq: None,
            last_seq: None,
            chain_head_hash: None,
        });
    }
    let first_seq = rows.first().map(|r| r.seq);
    let last_seq = rows.last().map(|r| r.seq);
    let chain_head = rows.last().map(|r| r.row_hash.clone());

    match &cfg.mode {
        ExportMode::Filesystem { dir } => {
            std::fs::create_dir_all(dir)?;
            let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S");
            let ndjson_path = dir.join(format!("audit-{ts}.ndjson"));
            let manifest_path = dir.join(format!("audit-{ts}.manifest.json"));

            // Write NDJSON.
            {
                use std::io::Write;
                let mut f = std::fs::File::create(&ndjson_path)?;
                for row in &rows {
                    let line = serde_json::json!({
                        "seq": row.seq,
                        "id": row.id,
                        "timestamp": row.timestamp,
                        "principal_key": row.principal_key,
                        "action": row.action,
                        "resource_type": row.resource_type,
                        "resource_id": row.resource_id,
                        "result": row.result,
                        "metadata": serde_json::from_str::<serde_json::Value>(&row.metadata_json)
                            .unwrap_or(serde_json::Value::Null),
                        "prev_hash": row.prev_hash,
                        "row_hash": row.row_hash,
                    });
                    writeln!(f, "{line}")?;
                }
                f.sync_all()?;
            }

            // Manifest.
            let manifest = serde_json::json!({
                "rows_exported": rows.len(),
                "first_seq": first_seq,
                "last_seq": last_seq,
                "chain_head_hash": chain_head,
                "exported_at": chrono::Utc::now().to_rfc3339(),
                "ndjson_file": ndjson_path.file_name().and_then(|n| n.to_str()),
            });
            std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;

            // NOTE: A-15 — `chattr +i` was removed. It silently fails in
            // non-root containers (Spacebot's Talos deployment runs as
            // non-root), defeating the tamper-evidence claim. Filesystem
            // mode is dev-only and documented as NOT tamper-evident.
            // Production must use `ExportMode::S3` or `ExportMode::HttpSiem`.
        }
        ExportMode::S3 {
            endpoint: _,
            bucket: _,
            prefix: _,
            region: _,
            access_key_id: _,
            secret_access_key: _,
            retention_days: _,
            object_lock_mode: _,
        } => {
            // Upload NDJSON + manifest to S3-compatible storage with
            // Object Lock retention. Uses `aws-sdk-s3` or similar.
            // Pseudocode (real impl in src/audit/s3_export.rs):
            //   1. Build aws_sdk_s3::Client with endpoint + region +
            //      StaticCredentials(access_key_id, secret_access_key).
            //   2. PutObject `prefix/audit-{ts}.ndjson` with
            //      object_lock_mode, object_lock_retain_until_date =
            //      now + retention_days.
            //   3. PutObject `prefix/audit-{ts}.manifest.json` same way.
            // Handle network errors with exponential backoff up to 3
            // attempts; leave source rows intact on failure (do NOT
            // advance `last_exported_seq` until success).
            anyhow::bail!(
                "S3 export implementation lives in src/audit/s3_export.rs; \
                 this stub is a reminder to wire it up before Phase 10"
            );
        }
        ExportMode::HttpSiem {
            endpoint: _,
            auth_header: _,
            batch_size: _,
        } => {
            // POST NDJSON in batches. Each batch is one HTTP request.
            // If all batches 2xx, advance `last_exported_seq`. On any
            // failure, advance to the last successful seq only.
            anyhow::bail!(
                "SIEM HTTP export implementation lives in src/audit/siem_export.rs; \
                 this stub is a reminder to wire it up before Phase 10"
            );
        }
    }

    // A-14: advance `last_exported_seq` for this mode after successful export.
    if let Some(last) = last_seq {
        match pool {
            DbPool::Sqlite(p) => {
                sqlx::query(
                    r#"
                    INSERT INTO audit_export_state (export_mode, last_exported_seq, last_exported_at, last_exported_row_hash)
                    VALUES (?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), ?)
                    ON CONFLICT(export_mode) DO UPDATE SET
                        last_exported_seq = excluded.last_exported_seq,
                        last_exported_at = excluded.last_exported_at,
                        last_exported_row_hash = excluded.last_exported_row_hash,
                        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                    "#,
                )
                .bind(mode_kind)
                .bind(last)
                .bind(&chain_head)
                .execute(p)
                .await?;
            }
            DbPool::Postgres(p) => {
                sqlx::query(
                    r#"
                    INSERT INTO audit_export_state (export_mode, last_exported_seq, last_exported_at, last_exported_row_hash)
                    VALUES ($1, $2, to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'), $3)
                    ON CONFLICT(export_mode) DO UPDATE SET
                        last_exported_seq = excluded.last_exported_seq,
                        last_exported_at = excluded.last_exported_at,
                        last_exported_row_hash = excluded.last_exported_row_hash,
                        updated_at = to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
                    "#,
                )
                .bind(mode_kind)
                .bind(last)
                .bind(&chain_head)
                .execute(p)
                .await?;
            }
        }
    }

    Ok(ExportResult {
        rows_exported: rows.len(),
        first_seq,
        last_seq,
        chain_head_hash: chain_head,
    })
}
