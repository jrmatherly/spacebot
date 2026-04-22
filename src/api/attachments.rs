//! Attachment upload, serving, and listing endpoints + their shared
//! Phase-4 authz gate.
//!
//! All three endpoints (`upload_attachment`, `serve_attachment`,
//! `list_attachments`) consult `check_read_with_audit` or `check_write`
//! before touching the `saved_attachments` table on the agent-scoped
//! pool. Reads gate on the attachment's own `resource_ownership` row
//! (`resource_type = "saved_attachment"`, `resource_id = attachment.id`,
//! A-09 bare UUID). Writes (upload) pre-check `check_write("agent",
//! &agent_id)` since the caller must have write access to the target
//! agent's workspace before a new attachment row can be created for it,
//! then `.await set_ownership("saved_attachment", ...)` AFTER the insert
//! succeeds (A-12: a fire-and-forget `tokio::spawn` races the uploader's
//! immediate GET into a 404).
//!
//! Attachments carry parent-resource relationships: a `saved_attachment`
//! row can reference a `message_id` (portal history), a task, or a
//! memory. Authz here keys on the attachment's own ownership row, NOT
//! the parent. Only the per-attachment `resource_ownership` entry is
//! the source of truth. A future parent-inheritance policy would need
//! an explicit design decision; for Phase 4 we do not recurse through
//! parents.
//!
//! `list_attachments` is filtered by `channel_id` but keys the gate on
//! the path `agent_id` — the agent's ownership row is the authorisation
//! root for all attachments the agent owns. A Phase-5 TODO below tracks
//! per-row post-filtering once the audit log lands and can absorb the
//! N+1 cost.
//!
//! The ~45-line inline gate block mirrors `src/api/tasks.rs` and
//! `src/api/memories.rs` per Phase 4 PR 2 decision N1: single-file
//! grep-visibility beats DRY. Pool-None is always-on `tracing::error!`
//! plus feature-gated `spacebot_authz_skipped_total{handler="attachments"}`.
//! Metric label is uniformly `"attachments"` (the file family), never a
//! per-handler sub-label.

use super::state::ApiState;
use crate::agent::channel_attachments::persist_attachment_bytes;

use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use serde::{Deserialize, Serialize};
use sqlx::Row as _;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct AttachmentUploadResponse {
    id: String,
    original_filename: String,
    mime_type: String,
    size_bytes: u64,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct AttachmentInfo {
    id: String,
    original_filename: String,
    mime_type: String,
    size_bytes: u64,
    created_at: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct AttachmentListResponse {
    attachments: Vec<AttachmentInfo>,
}

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub(super) struct AttachmentServeQuery {
    /// When true, force Content-Disposition: attachment (download).
    #[serde(default)]
    download: bool,
    /// When true, serve a thumbnail-sized version (for display in the UI).
    /// Currently serves the full file; thumbnail generation is a future enhancement.
    #[serde(default)]
    #[allow(dead_code)]
    thumbnail: bool,
}

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub(super) struct AttachmentListQuery {
    /// Filter to attachments from a specific message.
    message_id: Option<String>,
    limit: Option<i64>,
}

// ---------------------------------------------------------------------------
// Upload
// ---------------------------------------------------------------------------

/// Upload a file attachment for a portal conversation.
///
/// The file is persisted to `workspace/saved/` and tracked in `saved_attachments`.
/// Returns an attachment ID to include in the subsequent message send request.
#[utoipa::path(
    post,
    path = "/agents/{agent_id}/channels/{channel_id}/attachments/upload",
    params(
        ("agent_id" = String, Path, description = "Agent ID"),
        ("channel_id" = String, Path, description = "Channel / conversation ID"),
    ),
    responses(
        (status = 200, body = AttachmentUploadResponse),
        (status = 400, description = "Invalid or empty file"),
        (status = 404, description = "Agent not found"),
        (status = 413, description = "File too large (max 50 MB)"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "portal",
)]
pub(super) async fn upload_attachment(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Path((agent_id, channel_id)): Path<(String, String)>,
    mut multipart: axum::extract::Multipart,
) -> Result<Json<AttachmentUploadResponse>, StatusCode> {
    const MAX_SIZE: usize = 50 * 1024 * 1024; // 50 MB

    // Phase 4 authz gate: uploading creates a new `saved_attachment` row
    // owned by the caller, but the pre-check rides the agent's ownership
    // row (a caller without write access to the agent must not be able
    // to stuff files into its workspace). Mirrors
    // `create_portal_conversation` in portal.rs.
    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let access = crate::auth::check_write(&pool, &auth_ctx, "agent", &agent_id)
            .await
            .map_err(|error| {
                tracing::warn!(
                    %error,
                    actor = %auth_ctx.principal_key(),
                    resource_type = "agent",
                    resource_id = %agent_id,
                    "authz check_write failed"
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if !access.is_allowed() {
            crate::auth::policy::fire_denied_audit(
                &state.audit,
                &auth_ctx,
                "agent",
                agent_id.as_str(),
            );
            return Err(access.to_status());
        }
    } else {
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["attachments"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            agent_id = %agent_id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let workspaces = state.agent_workspaces.load();
    let workspace = workspaces.get(&agent_id).ok_or(StatusCode::NOT_FOUND)?;
    let saved_dir = workspace.join("saved");

    let pools = state.agent_pools.load();
    let pool = pools.get(&agent_id).ok_or(StatusCode::NOT_FOUND)?;

    // Read the first file field from the multipart body.
    let mut field = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .ok_or(StatusCode::BAD_REQUEST)?;

    let original_filename = field.file_name().unwrap_or("upload").to_string();

    let content_type = field
        .content_type()
        .map(|ct| ct.to_string())
        .unwrap_or_else(|| {
            mime_guess::from_path(&original_filename)
                .first_or_octet_stream()
                .to_string()
        });

    // Read the body in chunks and abort early if the payload exceeds MAX_SIZE.
    // Without this, field.bytes() would buffer the entire upload into memory
    // before we can check the size.
    let mut bytes = Vec::new();
    loop {
        match field.chunk().await {
            Ok(Some(chunk)) => {
                if bytes.len() + chunk.len() > MAX_SIZE {
                    return Err(StatusCode::PAYLOAD_TOO_LARGE);
                }
                bytes.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            Err(error) => {
                tracing::warn!(%error, "failed to read upload chunk");
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    }

    if bytes.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let meta = persist_attachment_bytes(
        pool,
        &channel_id,
        &saved_dir,
        &original_filename,
        &content_type,
        &bytes,
    )
    .await
    .map_err(|error| {
        tracing::warn!(%error, "failed to persist portal attachment");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // A-12: `.await` set_ownership. A fire-and-forget `tokio::spawn` here
    // races the uploader's immediate GET /attachments/{id} into a 404.
    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        crate::auth::repository::set_ownership(
            &pool,
            "saved_attachment",
            &meta.id,
            None,
            &auth_ctx.principal_key(),
            crate::auth::principals::Visibility::Personal,
            None,
        )
        .await
        .map_err(|error| {
            tracing::error!(
                %error,
                attachment_id = %meta.id,
                "failed to register attachment ownership"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    } else {
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["attachments"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            attachment_id = %meta.id,
            "set_ownership skipped: instance_pool not attached"
        );
    }

    Ok(Json(AttachmentUploadResponse {
        id: meta.id,
        original_filename: meta.filename,
        mime_type: meta.mime_type,
        size_bytes: meta.size_bytes,
    }))
}

// ---------------------------------------------------------------------------
// Serve
// ---------------------------------------------------------------------------

/// Serve a saved attachment file.
///
/// Reads the file from disk with the correct Content-Type.
/// Use `?download=true` to force a download prompt.
/// Use `?thumbnail=true` to request a thumbnail (currently serves full file).
#[utoipa::path(
    get,
    path = "/agents/{agent_id}/attachments/{attachment_id}",
    params(
        ("agent_id" = String, Path, description = "Agent ID"),
        ("attachment_id" = String, Path, description = "Attachment ID"),
        AttachmentServeQuery,
    ),
    responses(
        (status = 200, description = "File content"),
        (status = 404, description = "Attachment not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "portal",
)]
pub(super) async fn serve_attachment(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Path((agent_id, attachment_id)): Path<(String, String)>,
    Query(query): Query<AttachmentServeQuery>,
) -> Result<Response, StatusCode> {
    // Phase 4 authz gate: keys on the attachment's own ownership row
    // (resource_type = "saved_attachment", resource_id = attachment_id)
    // per A-09 bare-UUID. Parent relationships (message_id, task,
    // memory) are NOT consulted here; the per-attachment ownership
    // entry is the source of truth. The gate runs BEFORE the disk read
    // so a non-owner never sees whether the file exists on disk.
    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let (access, admin_override) = crate::auth::check_read_with_audit(
            &pool,
            &auth_ctx,
            "saved_attachment",
            &attachment_id,
        )
        .await
        .map_err(|error| {
            tracing::warn!(
                %error,
                actor = %auth_ctx.principal_key(),
                resource_type = "saved_attachment",
                resource_id = %attachment_id,
                "authz check_read_with_audit failed"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        if !access.is_allowed() {
            crate::auth::policy::fire_denied_audit(
                &state.audit,
                &auth_ctx,
                "saved_attachment",
                attachment_id.as_str(),
            );
            return Err(access.to_status());
        }
        if admin_override {
            crate::auth::policy::fire_admin_read_audit(
                &state.audit,
                &auth_ctx,
                "saved_attachment",
                attachment_id.as_str(),
            );
        }
    } else {
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["attachments"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            attachment_id = %attachment_id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let pools = state.agent_pools.load();
    let pool = pools.get(&agent_id).ok_or(StatusCode::NOT_FOUND)?;

    let workspaces = state.agent_workspaces.load();
    let workspace = workspaces.get(&agent_id).ok_or(StatusCode::NOT_FOUND)?;
    let saved_dir = workspace.join("saved");

    let row = sqlx::query(
        "SELECT original_filename, saved_filename, mime_type \
         FROM saved_attachments WHERE id = ?",
    )
    .bind(&attachment_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        tracing::warn!(%error, "failed to query attachment");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    let original_filename: String = row.try_get("original_filename").map_err(|error| {
        tracing::error!(%error, %attachment_id, "saved_attachments row missing original_filename");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let mime_type: String = row.try_get("mime_type").map_err(|error| {
        tracing::error!(%error, %attachment_id, "saved_attachments row missing mime_type");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let saved_filename: String = row.try_get("saved_filename").map_err(|error| {
        tracing::error!(%error, %attachment_id, "saved_attachments row missing saved_filename");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Re-derive the path from the agent's saved/ directory instead of trusting
    // the disk_path column, which could read arbitrary files if the row is
    // corrupted. saved_filename is sanitized on insert but we also reject
    // anything that escapes saved_dir.
    let resolved_path = saved_dir.join(&saved_filename);
    if !resolved_path.starts_with(&saved_dir) {
        tracing::error!(%attachment_id, %saved_filename, "attachment path escapes saved dir");
        return Err(StatusCode::NOT_FOUND);
    }

    let bytes = tokio::fs::read(&resolved_path).await.map_err(|error| {
        tracing::warn!(%error, path = %resolved_path.display(), "attachment file missing from disk");
        StatusCode::NOT_FOUND
    })?;

    let disposition = if query.download {
        format!(
            "attachment; filename=\"{}\"",
            sanitize_header_value(&original_filename)
        )
    } else {
        format!(
            "inline; filename=\"{}\"",
            sanitize_header_value(&original_filename)
        )
    };

    let response = Response::builder()
        .header(header::CONTENT_TYPE, &mime_type)
        .header(header::CONTENT_DISPOSITION, &disposition)
        .header(header::CONTENT_LENGTH, bytes.len())
        .body(Body::from(bytes))
        .map_err(|error| {
            tracing::error!(%error, "failed to build attachment response");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(response)
}

// ---------------------------------------------------------------------------
// List
// ---------------------------------------------------------------------------

/// List saved attachments for a channel.
#[utoipa::path(
    get,
    path = "/agents/{agent_id}/channels/{channel_id}/attachments",
    params(
        ("agent_id" = String, Path, description = "Agent ID"),
        ("channel_id" = String, Path, description = "Channel ID"),
        AttachmentListQuery,
    ),
    responses(
        (status = 200, body = AttachmentListResponse),
        (status = 404, description = "Agent not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "portal",
)]
pub(super) async fn list_attachments(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Path((agent_id, channel_id)): Path<(String, String)>,
    Query(query): Query<AttachmentListQuery>,
) -> Result<Json<AttachmentListResponse>, StatusCode> {
    // Phase 4 authz gate: listings are scoped to a single agent (the
    // path `agent_id`). Gate on the agent's ownership row; per-row
    // post-filtering on each returned attachment is a Phase-5 TODO
    // below once the audit log can absorb the N+1 cost.
    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let (access, admin_override) =
            crate::auth::check_read_with_audit(&pool, &auth_ctx, "agent", &agent_id)
                .await
                .map_err(|error| {
                    tracing::warn!(
                        %error,
                        actor = %auth_ctx.principal_key(),
                        resource_type = "agent",
                        resource_id = %agent_id,
                        "authz check_read_with_audit failed"
                    );
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
        if !access.is_allowed() {
            crate::auth::policy::fire_denied_audit(
                &state.audit,
                &auth_ctx,
                "agent",
                agent_id.as_str(),
            );
            return Err(access.to_status());
        }
        if admin_override {
            crate::auth::policy::fire_admin_read_audit(
                &state.audit,
                &auth_ctx,
                "agent",
                agent_id.as_str(),
            );
        }
    } else {
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["attachments"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            agent_id = %agent_id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }
    // TODO(phase-5): per-row `check_read` over each returned attachment
    // once the audit log lands and can absorb the N+1 emission cost.
    // Today a caller with agent access sees every attachment the agent
    // holds in the channel; that matches the Phase-4 model where
    // attachments inherit agent-scope reachability but a stricter
    // per-attachment policy is a Phase-5 tightening.

    let pools = state.agent_pools.load();
    let pool = pools.get(&agent_id).ok_or(StatusCode::NOT_FOUND)?;

    let limit = query.limit.unwrap_or(100).min(500);

    let rows = if let Some(ref message_id) = query.message_id {
        sqlx::query(
            "SELECT id, original_filename, mime_type, size_bytes, created_at \
             FROM saved_attachments \
             WHERE channel_id = ? AND message_id = ? \
             ORDER BY created_at ASC LIMIT ?",
        )
        .bind(&channel_id)
        .bind(message_id)
        .bind(limit)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query(
            "SELECT id, original_filename, mime_type, size_bytes, created_at \
             FROM saved_attachments \
             WHERE channel_id = ? \
             ORDER BY created_at ASC LIMIT ?",
        )
        .bind(&channel_id)
        .bind(limit)
        .fetch_all(pool)
        .await
    }
    .map_err(|error| {
        tracing::warn!(%error, "failed to list attachments");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let attachments = rows
        .into_iter()
        .map(|row| {
            let id: String = row.try_get("id")?;
            let original_filename: String = row.try_get("original_filename")?;
            let mime_type: String = row.try_get("mime_type")?;
            let size_bytes: i64 = row.try_get("size_bytes")?;
            let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at")?;
            Ok::<_, sqlx::Error>(AttachmentInfo {
                id,
                original_filename,
                mime_type,
                size_bytes: u64::try_from(size_bytes).unwrap_or(0),
                created_at: created_at.to_rfc3339(),
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            tracing::error!(%error, "saved_attachments row missing expected column");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(AttachmentListResponse { attachments }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Strip characters that could break HTTP header values.
fn sanitize_header_value(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii() && *c != '"' && *c != '\\' && *c != '\r' && *c != '\n')
        .collect()
}
