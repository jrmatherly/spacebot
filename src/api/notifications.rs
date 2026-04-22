//! Notification HTTP handlers + shared Phase-4 authz gate.
//!
//! All per-notification write endpoints (`mark_read`, `dismiss_notification`)
//! consult `check_write` with `resource_type = "notification"` before
//! mutating the notification row. Access keys on the notification's UUID
//! `id` directly (A-09: bare UUID, no slug→UUID indirection — the URL
//! path already carries the UUID), so there is no fetch-before-gate
//! step; a denied write collapses with the `store.mark_read(...) ==
//! false` 404 to the same client-visible shape.
//!
//! `list_notifications` gates only when the optional `agent_id` filter
//! is provided (mirrors `list_tasks` / `list_memories`: the filter
//! identifies a single agent resource). Listings without an agent
//! filter, plus `unread_count`, `mark_all_read`, and `dismiss_read`,
//! carry a Phase-5 TODO: those return or mutate every notification the
//! instance holds, which requires per-row gating or an admin-only
//! contract once the audit log lands. Notifications have no user-facing
//! POST endpoint: all creations happen server-side via
//! `ApiState::emit_notification`. Per the Phase-4 no-auto-broadening
//! backfill policy documented in
//! `docs/design-docs/entra-backfill-strategy.md` (§11.3), no bulk
//! ownership rows are written for pre-existing notifications; the
//! Phase-10 orphan sweep is the designated broadening path.
//!
//! The ~45-line inline gate block mirrors `src/api/memories.rs` and
//! `src/api/tasks.rs` per Phase 4 PR 2 decision N1: single-file
//! grep-visibility beats DRY. Pool-None is always-on `tracing::error!`
//! plus feature-gated
//! `spacebot_authz_skipped_total{handler="notifications"}`. The metric
//! label is the file resource family (`"notifications"`), never a
//! per-handler sub-label, which keeps cardinality flat.
//!
//! Pool-None warn secondary field differs by call-site context:
//! `list_notifications` logs `agent_id` (the filter that identifies
//! the resource), while `mark_read` / `dismiss_notification` log
//! `notification_id` (the URL-path identifier of the single resource
//! being mutated). This divergence is intentional: the secondary field
//! names the resource key actually observable at the call site.
//!
//! Phase 5 replaces the `tracing::info!` admin-override path with an
//! `AuditAppender::append` call against the hash-chained audit log.

use super::state::{ApiEvent, ApiState};
use crate::notifications::{Notification, NotificationFilter, NotificationKind, NotificationStore};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub(super) struct ListNotificationsQuery {
    /// "unread" returns only unread notifications; anything else returns all.
    #[serde(default)]
    pub filter: Option<String>,
    /// Filter by agent id.
    pub agent_id: Option<String>,
    /// Filter by kind: "task_approval", "worker_failed", "cortex_observation".
    pub kind: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    50
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(super) struct NotificationsResponse {
    pub notifications: Vec<Notification>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(super) struct UnreadCountResponse {
    pub count: i64,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn get_notification_store(state: &ApiState) -> Result<Arc<NotificationStore>, StatusCode> {
    state
        .notification_store
        .load()
        .as_ref()
        .clone()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)
}

fn parse_kind(value: Option<&str>) -> Option<NotificationKind> {
    match value? {
        "task_approval" => Some(NotificationKind::TaskApproval),
        "worker_failed" => Some(NotificationKind::WorkerFailed),
        "cortex_observation" => Some(NotificationKind::CortexObservation),
        _ => None,
    }
}

fn broadcast_updated(state: &ApiState, id: &str, read: bool, dismissed: bool) {
    state
        .event_tx
        .send(ApiEvent::NotificationUpdated {
            id: id.to_string(),
            read,
            dismissed,
        })
        .ok();
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /notifications` — list notifications with optional filters.
#[utoipa::path(
    get,
    path = "/notifications",
    params(ListNotificationsQuery),
    responses(
        (status = 200, body = NotificationsResponse),
        (status = 503, description = "Notification store not initialized"),
    ),
    tag = "notifications",
)]
pub(super) async fn list_notifications(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Query(query): Query<ListNotificationsQuery>,
) -> Result<Json<NotificationsResponse>, StatusCode> {
    // Phase 4 authz gate: a list scoped to a single agent (`agent_id`)
    // rides that agent's ownership row, matching `list_tasks` /
    // `list_memories`. Without this, a caller could enumerate another
    // user's notifications by passing `agent_id=<their-agent>` — the SQL
    // filter narrows the result set, but the rows inside still belong to
    // the other user.
    if let Some(agent_id) = query.agent_id.as_deref() {
        if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
            let (access, admin_override) =
                crate::auth::check_read_with_audit(&pool, &auth_ctx, "agent", agent_id)
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
                return Err(access.to_status());
            }
            if admin_override {
                tracing::info!(
                    actor = %auth_ctx.principal_key(),
                    resource_type = "agent",
                    resource_id = %agent_id,
                    "admin_read override (audit event queued for Phase 5)"
                );
            }
        } else {
            #[cfg(feature = "metrics")]
            crate::telemetry::Metrics::global()
                .authz_skipped_total
                .with_label_values(&["notifications"])
                .inc();
            tracing::error!(
                actor = %auth_ctx.principal_key(),
                agent_id = %agent_id,
                "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
            );
        }
    }
    // TODO(phase-5): gate the no-filter listing path (currently returns
    // every notification the instance holds to any authenticated caller).
    // The correct fix is per-row check_read once the audit log lands and
    // can absorb the N+1 audit emission cost; an admin-only guard here
    // would be an acceptable interim tightening.

    let store = get_notification_store(&state)?;
    let unread_only = query.filter.as_deref() == Some("unread");
    let kind = parse_kind(query.kind.as_deref());

    let notifications = store
        .list(NotificationFilter {
            unread_only,
            include_dismissed: false,
            agent_id: query.agent_id,
            kind,
            limit: Some(query.limit.clamp(1, 500)),
            offset: Some(query.offset),
        })
        .await
        .map_err(|error| {
            tracing::warn!(%error, "failed to list notifications");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(NotificationsResponse { notifications }))
}

/// `GET /notifications/unread_count` — count unread, undismissed notifications.
#[utoipa::path(
    get,
    path = "/notifications/unread_count",
    responses(
        (status = 200, body = UnreadCountResponse),
        (status = 503, description = "Notification store not initialized"),
    ),
    tag = "notifications",
)]
pub(super) async fn unread_count(
    State(state): State<Arc<ApiState>>,
    _auth_ctx: crate::auth::context::AuthContext,
) -> Result<Json<UnreadCountResponse>, StatusCode> {
    // TODO(phase-5): this returns a global unread count over every
    // notification row. Per-row gating or an admin-only contract lands
    // once the audit log can absorb the N+1 emission cost. For now the
    // endpoint requires authentication but does not authorize.
    let store = get_notification_store(&state)?;
    let count = store.unread_count().await.map_err(|error| {
        tracing::warn!(%error, "failed to count unread notifications");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(UnreadCountResponse { count }))
}

/// `POST /notifications/{id}/read` — mark a single notification as read.
#[utoipa::path(
    post,
    path = "/notifications/{id}/read",
    params(("id" = String, Path, description = "Notification id")),
    responses(
        (status = 204, description = "Marked as read"),
        (status = 404, description = "Not found or already read"),
        (status = 503, description = "Notification store not initialized"),
    ),
    tag = "notifications",
)]
pub(super) async fn mark_read(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    // Gate on the notification's UUID directly per A-09 (the URL path
    // already carries the UUID, no slug→UUID indirection). NotOwned 404
    // and "not found / already read" 404 collapse to the same
    // client-visible shape, so no fetch-before-gate is needed.
    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let access = crate::auth::check_write(&pool, &auth_ctx, "notification", &id)
            .await
            .map_err(|error| {
                tracing::warn!(
                    %error,
                    actor = %auth_ctx.principal_key(),
                    resource_type = "notification",
                    resource_id = %id,
                    "authz check_write failed"
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if !access.is_allowed() {
            return Err(access.to_status());
        }
    } else {
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["notifications"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            notification_id = %id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let store = get_notification_store(&state)?;
    let updated = store.mark_read(&id).await.map_err(|error| {
        tracing::warn!(%error, %id, "failed to mark notification read");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if !updated {
        return Err(StatusCode::NOT_FOUND);
    }
    broadcast_updated(&state, &id, true, false);
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /notifications/{id}/dismiss` — dismiss a single notification.
#[utoipa::path(
    post,
    path = "/notifications/{id}/dismiss",
    params(("id" = String, Path, description = "Notification id")),
    responses(
        (status = 204, description = "Dismissed"),
        (status = 404, description = "Not found or already dismissed"),
        (status = 503, description = "Notification store not initialized"),
    ),
    tag = "notifications",
)]
pub(super) async fn dismiss_notification(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    // Gate on the notification's UUID directly per A-09 (the URL path
    // already carries the UUID, no slug→UUID indirection). NotOwned 404
    // and "not found / already dismissed" 404 collapse to the same
    // client-visible shape.
    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let access = crate::auth::check_write(&pool, &auth_ctx, "notification", &id)
            .await
            .map_err(|error| {
                tracing::warn!(
                    %error,
                    actor = %auth_ctx.principal_key(),
                    resource_type = "notification",
                    resource_id = %id,
                    "authz check_write failed"
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if !access.is_allowed() {
            return Err(access.to_status());
        }
    } else {
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["notifications"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            notification_id = %id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let store = get_notification_store(&state)?;
    let updated = store.dismiss(&id).await.map_err(|error| {
        tracing::warn!(%error, %id, "failed to dismiss notification");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if !updated {
        return Err(StatusCode::NOT_FOUND);
    }
    broadcast_updated(&state, &id, true, true);
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /notifications/read_all` — mark all undismissed notifications as read.
#[utoipa::path(
    post,
    path = "/notifications/read_all",
    responses(
        (status = 204, description = "All marked as read"),
        (status = 503, description = "Notification store not initialized"),
    ),
    tag = "notifications",
)]
pub(super) async fn mark_all_read(
    State(state): State<Arc<ApiState>>,
    _auth_ctx: crate::auth::context::AuthContext,
) -> Result<StatusCode, StatusCode> {
    // TODO(phase-5): this mutates every unread row in the instance.
    // Replace with a per-row `check_write` sweep once the audit log can
    // absorb the emission cost, or narrow to an admin-only contract.
    let store = get_notification_store(&state)?;
    store.mark_all_read().await.map_err(|error| {
        tracing::warn!(%error, "failed to mark all notifications read");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    // Broadcast a generic update so connected clients re-fetch
    state
        .event_tx
        .send(ApiEvent::NotificationUpdated {
            id: String::new(),
            read: true,
            dismissed: false,
        })
        .ok();
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /notifications/dismiss_read` — dismiss all already-read notifications.
#[utoipa::path(
    post,
    path = "/notifications/dismiss_read",
    responses(
        (status = 204, description = "Read notifications dismissed"),
        (status = 503, description = "Notification store not initialized"),
    ),
    tag = "notifications",
)]
pub(super) async fn dismiss_read(
    State(state): State<Arc<ApiState>>,
    _auth_ctx: crate::auth::context::AuthContext,
) -> Result<StatusCode, StatusCode> {
    // TODO(phase-5): this mutates every read row in the instance.
    // Replace with a per-row `check_write` sweep once the audit log can
    // absorb the emission cost, or narrow to an admin-only contract.
    let store = get_notification_store(&state)?;
    store.dismiss_read().await.map_err(|error| {
        tracing::warn!(%error, "failed to dismiss read notifications");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    state
        .event_tx
        .send(ApiEvent::NotificationUpdated {
            id: String::new(),
            read: true,
            dismissed: true,
        })
        .ok();
    Ok(StatusCode::NO_CONTENT)
}
