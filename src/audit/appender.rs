//! `AuditAppender` — the only production writer to `audit_events`. Holds
//! a Tokio mutex so chained writes are serialized. The transaction is
//! narrow (SELECT prior row + INSERT new row + commit) so contention is
//! proportional to audit volume, not request volume.
//!
//! Per-method dispatch on `Arc<DbPool>` per Phase 11.2. Both backends store
//! audit_events with identical column types (TEXT for ISO-8601 timestamps,
//! BIGINT for seq) so the `AuditRow` `FromRow` derive works generically;
//! only placeholder syntax (`?` vs `$N`) and transaction begin diverge.

use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::audit::types::{AuditEvent, AuditRow, canonical_bytes, sha256_hex};
use crate::db::DbPool;

pub struct AuditAppender {
    pool: Arc<DbPool>,
    write_mutex: Arc<Mutex<()>>,
}

pub struct ChainVerifyResult {
    pub valid: bool,
    pub first_mismatch_seq: Option<i64>,
    pub total_rows: i64,
}

impl AuditAppender {
    /// A-13: `pub(crate)` to enforce singleton discipline. Production callers
    /// get an `Arc<AuditAppender>` from `ApiState::audit()`. Test callers
    /// use `new_for_tests` below.
    ///
    /// The singleton-via-ApiState pattern prevents two concurrent appenders
    /// from racing on prior-row reads and both writing the same `seq`
    /// (UNIQUE INDEX catches it, but the second writer's error is swallowed
    /// by the fire-and-forget `tokio::spawn` in the middleware → silent
    /// audit gap).
    pub(crate) fn new(pool: Arc<DbPool>) -> Self {
        Self {
            pool,
            write_mutex: Arc::new(Mutex::new(())),
        }
    }

    /// Test-only constructor. Plain `pub` (no `cfg` gate) because
    /// integration tests under `tests/*.rs` are separate compilation units
    /// that do NOT see `cfg(test)`. Precedent: `src/auth.rs::testing` is
    /// plain `pub mod`, `ApiState::new_test_state_with_mock_entra` is plain
    /// `pub`. `#[doc(hidden)]` keeps it out of rendered rustdoc so the
    /// surface still LOOKS internal, but integration tests can call it.
    ///
    /// Option Fixture-A bridge: accepts the legacy `SqlitePool` parameter
    /// and wraps it into `Arc<DbPool::Sqlite(...))` internally so the 14
    /// audit-domain test files (tests/audit_chain.rs, audit_export.rs,
    /// audit_scrubbing.rs, api_admin_audit.rs, etc.) keep working without
    /// per-call-site edits. The full Phase 11.2 widen-to-Arc<DbPool> on the
    /// test surface lands when ApiState.instance_pool widens (Task 13).
    #[doc(hidden)]
    pub fn new_for_tests(pool: SqlitePool) -> Self {
        Self::new(Arc::new(DbPool::Sqlite(pool)))
    }

    pub async fn append(&self, event: AuditEvent) -> sqlx::Result<AuditRow> {
        let _guard = self.write_mutex.lock().await;

        // Scrub any secret that may have leaked into metadata BEFORE we
        // compute the canonical bytes. Phase 0 Task 0.6 installed the JWT
        // regex in `src/secrets/scrub.rs::LEAK_PATTERNS`. A-01: we use
        // `scrub_leaks` (1-arg), not `scrub_secrets` (2-arg exact-match).
        //
        // PR #106 I9: log fallback paths explicitly. serde_json::to_string on
        // a serde_json::Value cannot fail today (Value is always serializable),
        // so these tracing::warn! calls are defense-in-depth. If a future
        // refactor changes metadata's type to something fallible, operators
        // see the failure instead of silently getting "{}"/Null.
        let metadata_str =
            serde_json::to_string(&event.metadata).unwrap_or_else(|error| {
                tracing::warn!(%error, "metadata serialization failed; emitting empty object into scrubber");
                "{}".into()
            });
        let scrubbed_str = crate::secrets::scrub::scrub_leaks(&metadata_str);
        let metadata_scrubbed: serde_json::Value = serde_json::from_str(&scrubbed_str)
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "scrubbed metadata failed to re-parse as JSON; falling back to original");
                event.metadata.clone()
            });
        let event = AuditEvent {
            metadata: metadata_scrubbed,
            ..event
        };

        let id = uuid::Uuid::now_v7().to_string();
        let timestamp = chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();

        // Per-backend transaction. SELECT prior + INSERT new + commit. The
        // SELECT is identical SQL on both backends (TEXT/INTEGER columns);
        // INSERT placeholders diverge `?` vs `$N`.
        let (seq, prev_hash) = match &*self.pool {
            DbPool::Sqlite(p) => {
                let mut tx = p.begin().await?;

                let prior: Option<(i64, String)> = sqlx::query_as(
                    "SELECT seq, row_hash FROM audit_events ORDER BY seq DESC LIMIT 1",
                )
                .fetch_optional(&mut *tx)
                .await?;
                let (prev_seq, prev_hash) = prior.unwrap_or((0, "0".repeat(64)));
                let seq = prev_seq + 1;

                let canon = canonical_bytes(&event, seq, &timestamp, &prev_hash);
                let row_hash = sha256_hex(&canon);
                let metadata_json =
                    serde_json::to_string(&event.metadata).unwrap_or_else(|error| {
                        tracing::warn!(%error, "scrubbed metadata serialization failed; storing empty object");
                        "{}".into()
                    });

                sqlx::query(
                    r#"
                    INSERT INTO audit_events (
                        id, seq, timestamp, principal_key, principal_type, action,
                        resource_type, resource_id, result, source_ip, request_id,
                        metadata_json, prev_hash, row_hash
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
                )
                .bind(&id)
                .bind(seq)
                .bind(&timestamp)
                .bind(&event.principal_key)
                .bind(&event.principal_type)
                .bind(event.action.as_str())
                .bind(event.resource_type.as_deref())
                .bind(event.resource_id.as_deref())
                .bind(&event.result)
                .bind(event.source_ip.as_deref())
                .bind(event.request_id.as_deref())
                .bind(&metadata_json)
                .bind(&prev_hash)
                .bind(&row_hash)
                .execute(&mut *tx)
                .await?;

                tx.commit().await?;
                (seq, prev_hash)
            }
            DbPool::Postgres(p) => {
                let mut tx = p.begin().await?;

                let prior: Option<(i64, String)> = sqlx::query_as(
                    "SELECT seq, row_hash FROM audit_events ORDER BY seq DESC LIMIT 1",
                )
                .fetch_optional(&mut *tx)
                .await?;
                let (prev_seq, prev_hash) = prior.unwrap_or((0, "0".repeat(64)));
                let seq = prev_seq + 1;

                let canon = canonical_bytes(&event, seq, &timestamp, &prev_hash);
                let row_hash = sha256_hex(&canon);
                let metadata_json =
                    serde_json::to_string(&event.metadata).unwrap_or_else(|error| {
                        tracing::warn!(%error, "scrubbed metadata serialization failed; storing empty object");
                        "{}".into()
                    });

                sqlx::query(
                    r#"
                    INSERT INTO audit_events (
                        id, seq, timestamp, principal_key, principal_type, action,
                        resource_type, resource_id, result, source_ip, request_id,
                        metadata_json, prev_hash, row_hash
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
                "#,
                )
                .bind(&id)
                .bind(seq)
                .bind(&timestamp)
                .bind(&event.principal_key)
                .bind(&event.principal_type)
                .bind(event.action.as_str())
                .bind(event.resource_type.as_deref())
                .bind(event.resource_id.as_deref())
                .bind(&event.result)
                .bind(event.source_ip.as_deref())
                .bind(event.request_id.as_deref())
                .bind(&metadata_json)
                .bind(&prev_hash)
                .bind(&row_hash)
                .execute(&mut *tx)
                .await?;

                tx.commit().await?;
                (seq, prev_hash)
            }
        };

        // Recompute row_hash + metadata_json for the return value (cheap;
        // both arms produced the same values from the same inputs).
        let canon = canonical_bytes(&event, seq, &timestamp, &prev_hash);
        let row_hash = sha256_hex(&canon);
        let metadata_json = serde_json::to_string(&event.metadata).unwrap_or_else(|error| {
            tracing::warn!(%error, "scrubbed metadata serialization failed; storing empty object");
            "{}".into()
        });

        Ok(AuditRow {
            id,
            seq,
            timestamp,
            principal_key: event.principal_key,
            principal_type: event.principal_type,
            action: event.action.as_str().to_string(),
            resource_type: event.resource_type,
            resource_id: event.resource_id,
            result: event.result,
            source_ip: event.source_ip,
            request_id: event.request_id,
            metadata_json,
            prev_hash,
            row_hash,
        })
    }

    pub async fn verify_chain(&self) -> sqlx::Result<ChainVerifyResult> {
        // Hold the write mutex so concurrent appends do not race the
        // snapshot read. Auditors calling this while the daemon serves
        // live traffic must see a consistent view of the chain. Without
        // this guard, a row inserted mid-SELECT can make verify() return
        // spurious false-negatives. (Per 2026-04-22 Phase 5 audit IMPORTANT 8.)
        let _guard = self.write_mutex.lock().await;
        let rows: Vec<AuditRow> = match &*self.pool {
            DbPool::Sqlite(p) => {
                sqlx::query_as("SELECT * FROM audit_events ORDER BY seq")
                    .fetch_all(p)
                    .await?
            }
            DbPool::Postgres(p) => {
                sqlx::query_as("SELECT * FROM audit_events ORDER BY seq")
                    .fetch_all(p)
                    .await?
            }
        };
        let total = rows.len() as i64;
        let mut prev_hash = "0".repeat(64);
        for row in &rows {
            if row.prev_hash != prev_hash {
                return Ok(ChainVerifyResult {
                    valid: false,
                    first_mismatch_seq: Some(row.seq),
                    total_rows: total,
                });
            }
            let event = AuditEvent {
                principal_key: row.principal_key.clone(),
                principal_type: row.principal_type.clone(),
                action: match row.action.as_str() {
                    "auth_success" => crate::audit::types::AuditAction::AuthSuccess,
                    "auth_failure" => crate::audit::types::AuditAction::AuthFailure,
                    "resource_create" => crate::audit::types::AuditAction::ResourceCreate,
                    "resource_read" => crate::audit::types::AuditAction::ResourceRead,
                    "resource_write" => crate::audit::types::AuditAction::ResourceWrite,
                    "resource_delete" => crate::audit::types::AuditAction::ResourceDelete,
                    "admin_read" => crate::audit::types::AuditAction::AdminRead,
                    "admin_write" => crate::audit::types::AuditAction::AdminWrite,
                    "admin_claim_resource" => crate::audit::types::AuditAction::AdminClaimResource,
                    "authz_denied" => crate::audit::types::AuditAction::AuthzDenied,
                    "orphan_detected" => crate::audit::types::AuditAction::OrphanDetected,
                    "export_run" => crate::audit::types::AuditAction::ExportRun,
                    _ => {
                        return Ok(ChainVerifyResult {
                            valid: false,
                            first_mismatch_seq: Some(row.seq),
                            total_rows: total,
                        });
                    }
                },
                resource_type: row.resource_type.clone(),
                resource_id: row.resource_id.clone(),
                result: row.result.clone(),
                source_ip: row.source_ip.clone(),
                request_id: row.request_id.clone(),
                metadata: serde_json::from_str(&row.metadata_json)
                    .unwrap_or(serde_json::Value::Null),
            };
            let computed = sha256_hex(&canonical_bytes(
                &event,
                row.seq,
                &row.timestamp,
                &row.prev_hash,
            ));
            if computed != row.row_hash {
                return Ok(ChainVerifyResult {
                    valid: false,
                    first_mismatch_seq: Some(row.seq),
                    total_rows: total,
                });
            }
            prev_hash = row.row_hash.clone();
        }
        Ok(ChainVerifyResult {
            valid: true,
            first_mismatch_seq: None,
            total_rows: total,
        })
    }
}
