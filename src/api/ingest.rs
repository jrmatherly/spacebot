//! Ingestion file HTTP handlers + shared Phase-4 authz gate.
//!
//! All three endpoints (`list_ingest_files`, `upload_ingest_file`,
//! `delete_ingest_file`) consult the Phase-4 authz helpers before
//! touching the per-agent SQLite pool or workspace on disk. Two resource
//! families are in play:
//!
//! - **Agent-scoped authz** (`resource_type = "agent"`) for handlers
//!   that operate on an entire agent's ingest directory: listing all
//!   files for an agent (`list_ingest_files`) and uploading new files
//!   into an agent's knowledge base (`upload_ingest_file`). These
//!   mirror the `list_tasks` / `list_memories` pattern: an agent-scoped
//!   filter identifies a single agent resource and rides that agent's
//!   ownership row.
//! - **File-scoped authz** (`resource_type = "ingestion_file"`) for
//!   handlers that target a single file by its content hash
//!   (`delete_ingest_file`). Access keys on the bare `content_hash`
//!   (A-09: the stable identifier the `ingestion_files` table uses as
//!   its primary key, no sigil'd prefix).
//!
//! `upload_ingest_file` `.await`s `set_ownership` with
//! `resource_type = "ingestion_file"` AFTER each file's `INSERT OR
//! IGNORE` succeeds. The `.await` is load-bearing (A-12): a
//! `tokio::spawn` fire-and-forget would race a subsequent
//! `DELETE /agents/ingest/files?content_hash=...` from the same user
//! into a NotOwned 404. The upload handler also writes through a
//! `Visibility::Personal` ownership row by default; shared-visibility
//! promotion is a future UI decision, not a handler default.
//!
//! The ~45-line inline gate block mirrors `src/api/memories.rs` per
//! Phase 4 PR 2 decision N1: single-file grep-visibility beats DRY.
//! Pool-None is always-on `tracing::error!` + feature-gated
//! `spacebot_authz_skipped_total{handler="ingest"}`. Metric label is
//! the file resource family (ingest), never a per-handler sub-label,
//! so counter cardinality stays flat.
//!
//! Phase 5 replaces the `tracing::info!` admin-override path with an
//! `AuditAppender::append` call against the hash-chained audit log.
//! Until that lands, the tracing log is the operational record. A
//! Phase-5 TODO covers broad unfiltered file listings if any ever
//! land here; today every endpoint in this file is agent-scoped.

use super::state::ApiState;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct IngestFileInfo {
    content_hash: String,
    filename: String,
    file_size: i64,
    total_chunks: i64,
    chunks_completed: i64,
    status: String,
    started_at: String,
    completed_at: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct IngestFilesResponse {
    files: Vec<IngestFileInfo>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct IngestUploadResponse {
    uploaded: Vec<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct IngestDeleteResponse {
    success: bool,
}

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub(super) struct IngestQuery {
    agent_id: String,
}

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub(super) struct IngestDeleteQuery {
    agent_id: String,
    content_hash: String,
}

/// List ingested files with progress info for in-progress ones.
#[utoipa::path(
    get,
    path = "/agents/ingest/files",
    params(
        ("agent_id" = String, Query, description = "Agent ID"),
    ),
    responses(
        (status = 200, body = IngestFilesResponse),
        (status = 404, description = "Agent not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "ingest",
)]
pub(super) async fn list_ingest_files(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Query(query): Query<IngestQuery>,
) -> Result<Json<IngestFilesResponse>, StatusCode> {
    use sqlx::Row as _;

    // Phase 4 authz gate: listing an agent's ingested files requires
    // read access to the agent resource itself (mirrors list_memories:
    // the filter identifies a single agent and rides its ownership row).
    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let (access, admin_override) =
            crate::auth::check_read_with_audit(&pool, &auth_ctx, "agent", &query.agent_id)
                .await
                .map_err(|error| {
                    tracing::warn!(
                        %error,
                        actor = %auth_ctx.principal_key(),
                        resource_type = "agent",
                        resource_id = %query.agent_id,
                        "authz check_read_with_audit failed"
                    );
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
        if !access.is_allowed() {
            crate::auth::policy::fire_denied_audit(
                &state.audit,
                &auth_ctx,
                "agent",
                query.agent_id.as_str(),
            );
            return Err(access.to_status());
        }
        if admin_override {
            crate::auth::policy::fire_admin_read_audit(
                &state.audit,
                &auth_ctx,
                "agent",
                query.agent_id.as_str(),
            );
        }
    } else {
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["ingest"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            agent_id = %query.agent_id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let pools = state.agent_pools.load();
    let pool = pools.get(&query.agent_id).ok_or(StatusCode::NOT_FOUND)?;

    let rows = sqlx::query(
        r#"
        SELECT f.content_hash, f.filename, f.file_size, f.total_chunks, f.status,
               f.started_at, f.completed_at,
               COALESCE(p.done, 0) as chunks_completed
        FROM ingestion_files f
        LEFT JOIN (
            SELECT content_hash, COUNT(*) as done
            FROM ingestion_progress
            GROUP BY content_hash
        ) p ON f.content_hash = p.content_hash
        ORDER BY f.started_at DESC
        LIMIT 100
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|error| {
        tracing::warn!(%error, "failed to list ingest files");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let files = rows
        .into_iter()
        .map(|row| IngestFileInfo {
            content_hash: row.get("content_hash"),
            filename: row.get("filename"),
            file_size: row.get("file_size"),
            total_chunks: row.get("total_chunks"),
            chunks_completed: row.get("chunks_completed"),
            status: row.get("status"),
            started_at: row.get("started_at"),
            completed_at: row.get("completed_at"),
        })
        .collect();

    Ok(Json(IngestFilesResponse { files }))
}

/// Upload one or more files to the agent's ingest directory.
#[utoipa::path(
    post,
    path = "/agents/ingest/files",
    params(
        ("agent_id" = String, Query, description = "Agent ID"),
    ),
    responses(
        (status = 200, body = IngestUploadResponse),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Agent not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "ingest",
)]
pub(super) async fn upload_ingest_file(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Query(query): Query<IngestQuery>,
    mut multipart: axum::extract::Multipart,
) -> Result<Json<IngestUploadResponse>, StatusCode> {
    // Phase 4 authz gate: uploading into an agent's ingest directory is
    // a write against that agent's knowledge base. Gate on the agent
    // resource; per-file `set_ownership("ingestion_file", ...)` calls
    // happen below after each row inserts (A-12).
    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let access = crate::auth::check_write(&pool, &auth_ctx, "agent", &query.agent_id)
            .await
            .map_err(|error| {
                tracing::warn!(
                    %error,
                    actor = %auth_ctx.principal_key(),
                    resource_type = "agent",
                    resource_id = %query.agent_id,
                    "authz check_write failed"
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if !access.is_allowed() {
            crate::auth::policy::fire_denied_audit(
                &state.audit,
                &auth_ctx,
                "agent",
                query.agent_id.as_str(),
            );
            return Err(access.to_status());
        }
    } else {
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["ingest"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            agent_id = %query.agent_id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let workspaces = state.agent_workspaces.load();
    let workspace = workspaces
        .get(&query.agent_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    let ingest_dir = workspace.join("ingest");

    tokio::fs::create_dir_all(&ingest_dir)
        .await
        .map_err(|error| {
            tracing::warn!(%error, "failed to create ingest directory");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut uploaded = Vec::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        let filename = field
            .file_name()
            .map(|n| n.to_string())
            .unwrap_or_else(|| format!("upload-{}.txt", uuid::Uuid::new_v4()));

        let data = field.bytes().await.map_err(|error| {
            tracing::warn!(%error, "failed to read upload field");
            StatusCode::BAD_REQUEST
        })?;

        if data.is_empty() {
            continue;
        }

        let safe_name = Path::new(&filename)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("upload.txt");

        let target = ingest_dir.join(safe_name);

        let target = if target.exists() {
            let stem = Path::new(safe_name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("upload");
            let ext = Path::new(safe_name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("txt");
            let unique = format!(
                "{}-{}.{}",
                stem,
                &uuid::Uuid::new_v4().to_string()[..8],
                ext
            );
            ingest_dir.join(unique)
        } else {
            target
        };

        tokio::fs::write(&target, &data).await.map_err(|error| {
            tracing::warn!(%error, path = %target.display(), "failed to write uploaded file");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        if let Ok(content) = std::str::from_utf8(&data) {
            let hash = crate::agent::ingestion::content_hash(content);
            let pools = state.agent_pools.load();
            if let Some(pool) = pools.get(&query.agent_id) {
                let file_size = data.len() as i64;
                // `INSERT OR IGNORE` intentionally absorbs unique-constraint
                // hits on re-upload (same content_hash). Any OTHER sqlx error
                // (disk full, WAL lock, FK failure) was previously swallowed
                // by `let _ = ...`. Now we warn so the failure is observable;
                // the upload still proceeds because the row is a tracking
                // aid, not a hard requirement for the downstream write.
                if let Err(error) = sqlx::query(
                    r#"
                    INSERT OR IGNORE INTO ingestion_files (content_hash, filename, file_size, total_chunks, status)
                    VALUES (?, ?, ?, 0, 'queued')
                    "#,
                )
                .bind(&hash)
                .bind(safe_name)
                .bind(file_size)
                .execute(pool)
                .await
                {
                    tracing::warn!(
                        %error,
                        content_hash = %hash,
                        agent_id = %query.agent_id,
                        "ingestion_files insert failed (non-dupe error)"
                    );
                }
            }

            // A-12: `.await` set_ownership AFTER the insert. A
            // fire-and-forget `tokio::spawn` here races the creator's
            // subsequent DELETE /agents/ingest/files?content_hash=... into
            // a NotOwned 404. Skipped silently when the instance_pool is
            // not attached; the pool-None branch above already emitted the
            // observability signal for this request.
            if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned()
                && let Err(error) = crate::auth::repository::set_ownership(
                    &pool,
                    "ingestion_file",
                    &hash,
                    Some(&query.agent_id),
                    &auth_ctx.principal_key(),
                    crate::auth::principals::Visibility::Personal,
                    None,
                )
                .await
            {
                tracing::error!(
                    %error,
                    content_hash = %hash,
                    "failed to register ingestion_file ownership"
                );
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }

        tracing::info!(
            agent_id = %query.agent_id,
            filename = %safe_name,
            bytes = data.len(),
            "file uploaded to ingest directory"
        );

        uploaded.push(safe_name.to_string());
    }

    Ok(Json(IngestUploadResponse { uploaded }))
}

/// Delete a completed ingestion file record from history.
#[utoipa::path(
    delete,
    path = "/agents/ingest/files",
    params(
        ("agent_id" = String, Query, description = "Agent ID"),
        ("content_hash" = String, Query, description = "Content hash of the file to delete"),
    ),
    responses(
        (status = 200, body = IngestDeleteResponse),
        (status = 404, description = "Agent not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "ingest",
)]
pub(super) async fn delete_ingest_file(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Query(query): Query<IngestDeleteQuery>,
) -> Result<Json<IngestDeleteResponse>, StatusCode> {
    // Phase 4 authz gate: deleting a specific ingestion file is a write
    // against the file resource itself (A-09: keyed on the bare
    // content_hash, which the ingestion_files table uses as its primary
    // key). A non-owner sees 404 per DenyReason::NotYours.
    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let access =
            crate::auth::check_write(&pool, &auth_ctx, "ingestion_file", &query.content_hash)
                .await
                .map_err(|error| {
                    tracing::warn!(
                        %error,
                        actor = %auth_ctx.principal_key(),
                        resource_type = "ingestion_file",
                        resource_id = %query.content_hash,
                        "authz check_write failed"
                    );
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
        if !access.is_allowed() {
            crate::auth::policy::fire_denied_audit(
                &state.audit,
                &auth_ctx,
                "ingestion_file",
                query.content_hash.as_str(),
            );
            return Err(access.to_status());
        }
    } else {
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["ingest"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            content_hash = %query.content_hash,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let pools = state.agent_pools.load();
    let pool = pools.get(&query.agent_id).ok_or(StatusCode::NOT_FOUND)?;

    sqlx::query("DELETE FROM ingestion_files WHERE content_hash = ?")
        .bind(&query.content_hash)
        .execute(pool)
        .await
        .map_err(|error| {
            tracing::warn!(%error, "failed to delete ingest file record");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(IngestDeleteResponse { success: true }))
}
