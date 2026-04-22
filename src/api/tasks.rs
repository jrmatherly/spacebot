//! Task HTTP handlers + shared Phase-4 authz gate.
//!
//! All read + write endpoints consult `check_read_with_audit` /
//! `check_write` with `resource_type = "task"` before touching the store.
//! Access keys on the task's UUID `id` (A-09: bare UUID, never
//! `task_number` or any sigil'd variant), even though the URL path uses
//! the human-friendly `task_number: i64`. Per-task handlers therefore
//! fetch the task once before the gate to resolve `task.id`; the
//! missing-row 404 collapses naturally with the `NotOwned` 404.
//!
//! `list_tasks` gates only when the optional `agent_id` filter is
//! provided (mirrors `list_memories`: the filter identifies a single
//! agent resource). Listings without an agent filter carry a Phase-5
//! TODO to post-filter by per-row `check_read` once the audit log
//! lands; a broad list-over-all-tasks gate doesn't fit the current
//! helper API.
//!
//! `create_task` skips the pre-check (nothing exists yet) and
//! `.await`s `set_ownership` AFTER the insert succeeds. The `.await`
//! is load-bearing (A-12): a `tokio::spawn` here races a subsequent
//! `GET /tasks/:number` from the creator into a 404, breaking
//! create-then-read UX.
//!
//! The ~45-line inline gate block mirrors `src/api/memories.rs` per
//! Phase 4 PR 2 decision N1: single-file grep-visibility beats DRY.
//! Pool-None is always-on `tracing::error!` + feature-gated
//! `spacebot_authz_skipped_total{handler="tasks"}`. Metric label is
//! the file resource family, never a per-handler sub-label.

use super::state::ApiState;
use crate::notifications::{NewNotification, NotificationKind, NotificationSeverity};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub(super) struct TaskListQuery {
    /// Convenience filter: matches tasks where owner OR assigned equals this value.
    #[serde(default)]
    agent_id: Option<String>,
    /// Filter by owner agent. Optional.
    #[serde(default)]
    owner_agent_id: Option<String>,
    /// Filter by assigned agent. Optional.
    #[serde(default)]
    assigned_agent_id: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    priority: Option<String>,
    #[serde(default)]
    created_by: Option<String>,
    #[serde(default = "default_task_limit")]
    limit: i64,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct CreateTaskRequest {
    /// Agent that owns (created) this task.
    owner_agent_id: String,
    /// Agent assigned to execute. Defaults to `owner_agent_id`.
    #[serde(default)]
    assigned_agent_id: Option<String>,
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    priority: Option<String>,
    #[serde(default)]
    subtasks: Vec<crate::tasks::TaskSubtask>,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
    #[serde(default)]
    source_memory_id: Option<String>,
    #[serde(default)]
    created_by: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct UpdateTaskRequest {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    priority: Option<String>,
    #[serde(default)]
    assigned_agent_id: Option<String>,
    #[serde(default)]
    subtasks: Option<Vec<crate::tasks::TaskSubtask>>,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
    #[serde(default)]
    complete_subtask: Option<usize>,
    #[serde(default)]
    worker_id: Option<String>,
    #[serde(default)]
    approved_by: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct ApproveRequest {
    #[serde(default)]
    approved_by: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct AssignRequest {
    assigned_agent_id: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct TaskListResponse {
    tasks: Vec<crate::tasks::Task>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct TaskResponse {
    task: crate::tasks::Task,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct TaskActionResponse {
    success: bool,
    message: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn default_task_limit() -> i64 {
    100
}

/// Extract the global task store, returning 503 if not yet initialized.
fn get_task_store(state: &ApiState) -> Result<Arc<crate::tasks::TaskStore>, StatusCode> {
    state
        .task_store
        .load()
        .as_ref()
        .clone()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)
}

fn parse_status(value: Option<&str>) -> Result<Option<crate::tasks::TaskStatus>, StatusCode> {
    match value {
        None => Ok(None),
        Some(value) => Ok(Some(
            crate::tasks::TaskStatus::parse(value).ok_or(StatusCode::BAD_REQUEST)?,
        )),
    }
}

fn parse_priority(value: Option<&str>) -> Result<Option<crate::tasks::TaskPriority>, StatusCode> {
    match value {
        None => Ok(None),
        Some(value) => Ok(Some(
            crate::tasks::TaskPriority::parse(value).ok_or(StatusCode::BAD_REQUEST)?,
        )),
    }
}

fn emit_task_event(state: &ApiState, task: &crate::tasks::Task, action: &str) {
    state
        .event_tx
        .send(super::state::ApiEvent::TaskUpdated {
            agent_id: task.assigned_agent_id.clone(),
            task_number: task.task_number,
            status: task.status.to_string(),
            action: action.to_string(),
        })
        .ok();
}

/// Emit a task_approval notification when a task enters the pending_approval state.
fn maybe_emit_approval_notification(state: &ApiState, task: &crate::tasks::Task) {
    if task.status != crate::tasks::TaskStatus::PendingApproval {
        return;
    }
    state.emit_notification(NewNotification {
        kind: NotificationKind::TaskApproval,
        severity: NotificationSeverity::Info,
        title: task.title.clone(),
        body: task.description.clone(),
        agent_id: Some(task.assigned_agent_id.clone()),
        related_entity_type: Some("task".to_string()),
        related_entity_id: Some(task.task_number.to_string()),
        action_url: Some(format!("/tasks/{}", task.task_number)),
        metadata: None,
    });
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /tasks` — list tasks with optional filters.
#[utoipa::path(
    get,
    path = "/tasks",
    params(TaskListQuery),
    responses(
        (status = 200, body = TaskListResponse),
        (status = 503, description = "Task store not initialized"),
    ),
    tag = "tasks",
)]
pub(super) async fn list_tasks(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Query(query): Query<TaskListQuery>,
) -> Result<Json<TaskListResponse>, StatusCode> {
    // Phase 4 authz gate: a list scoped to a single agent (`agent_id`)
    // rides that agent's ownership row, matching `list_memories`. The
    // Every agent-scoped filter (`agent_id`, `owner_agent_id`,
    // `assigned_agent_id`) is an agent-resource read. Gate each one that
    // was provided. Without this, a caller could enumerate another
    // user's tasks by passing `owner_agent_id=<their-agent>` — the SQL
    // filter narrows the result set, but the rows inside still belong
    // to the other user. Using the first agent-scoped filter keyed in
    // the request order keeps the gate deterministic.
    //
    // Unfiltered calls (no agent-scope filter at all) still carry the
    // Phase-5 TODO below: those listings return every task the instance
    // holds, which requires per-row gating or a caller-policy contract
    // that only an admin can list without a filter.
    let gate_agent_id: Option<&str> = query
        .agent_id
        .as_deref()
        .or(query.owner_agent_id.as_deref())
        .or(query.assigned_agent_id.as_deref());
    if let Some(agent_id) = gate_agent_id {
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
                .with_label_values(&["tasks"])
                .inc();
            tracing::error!(
                actor = %auth_ctx.principal_key(),
                agent_id = %agent_id,
                "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
            );
        }
    }
    // TODO(phase-5): gate the no-filter listing path (currently returns
    // every task the instance holds to any authenticated caller). The
    // correct fix is per-row check_read once the audit log lands and can
    // absorb the N+1 audit emission cost; in the interim an admin-only
    // guard here would be an acceptable tightening.

    let store = get_task_store(&state)?;

    let status = parse_status(query.status.as_deref())?;
    let priority = parse_priority(query.priority.as_deref())?;

    let tasks = store
        .list(crate::tasks::TaskListFilter {
            agent_id: query.agent_id,
            owner_agent_id: query.owner_agent_id,
            assigned_agent_id: query.assigned_agent_id,
            status,
            priority,
            created_by: query.created_by,
            limit: Some(query.limit.clamp(1, 500)),
        })
        .await
        .map_err(|error| {
            tracing::warn!(%error, "failed to list tasks");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(TaskListResponse { tasks }))
}

/// `GET /tasks/{number}` — get a task by globally unique number.
#[utoipa::path(
    get,
    path = "/tasks/{number}",
    params(
        ("number" = i64, Path, description = "Task number"),
    ),
    responses(
        (status = 200, body = TaskResponse),
        (status = 404, description = "Task not found"),
        (status = 503, description = "Task store not initialized"),
    ),
    tag = "tasks",
)]
pub(super) async fn get_task(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Path(number): Path<i64>,
) -> Result<Json<TaskResponse>, StatusCode> {
    let store = get_task_store(&state)?;

    // Fetch before the authz gate: task_number (URL) maps to task.id (UUID)
    // and the ownership row keys on the UUID per A-09. Missing-task 404
    // and NotOwned 404 collapse to the same client-visible shape.
    let task = store
        .get_by_number(number)
        .await
        .map_err(|error| {
            tracing::warn!(%error, task_number = number, "failed to get task");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let (access, admin_override) =
            crate::auth::check_read_with_audit(&pool, &auth_ctx, "task", &task.id)
                .await
                .map_err(|error| {
                    tracing::warn!(
                        %error,
                        actor = %auth_ctx.principal_key(),
                        resource_type = "task",
                        resource_id = %task.id,
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
                resource_type = "task",
                resource_id = %task.id,
                "admin_read override (audit event queued for Phase 5)"
            );
        }
    } else {
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["tasks"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            task_id = %task.id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    Ok(Json(TaskResponse { task }))
}

/// `POST /tasks` — create a task.
#[utoipa::path(
    post,
    path = "/tasks",
    request_body = CreateTaskRequest,
    responses(
        (status = 200, body = TaskResponse),
        (status = 400, description = "Invalid request"),
        (status = 503, description = "Task store not initialized"),
    ),
    tag = "tasks",
)]
pub(super) async fn create_task(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Json(request): Json<CreateTaskRequest>,
) -> Result<Json<TaskResponse>, StatusCode> {
    let store = get_task_store(&state)?;

    let status = crate::tasks::TaskStatus::PendingApproval;
    let priority =
        parse_priority(request.priority.as_deref())?.unwrap_or(crate::tasks::TaskPriority::Medium);

    let assigned = request
        .assigned_agent_id
        .unwrap_or_else(|| request.owner_agent_id.clone());

    let task = store
        .create(crate::tasks::CreateTaskInput {
            owner_agent_id: request.owner_agent_id,
            assigned_agent_id: assigned,
            title: request.title,
            description: request.description,
            status,
            priority,
            subtasks: request.subtasks,
            metadata: request.metadata.unwrap_or_else(|| serde_json::json!({})),
            source_memory_id: request.source_memory_id,
            created_by: request.created_by.unwrap_or_else(|| "human".to_string()),
        })
        .await
        .map_err(|error| {
            tracing::warn!(%error, "failed to create task");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // A-12: `.await` set_ownership. A fire-and-forget `tokio::spawn` here
    // races the creator's subsequent GET /tasks/{number} into a 404.
    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        crate::auth::repository::set_ownership(
            &pool,
            "task",
            &task.id,
            None,
            &auth_ctx.principal_key(),
            crate::auth::principals::Visibility::Personal,
            None,
        )
        .await
        .map_err(|error| {
            tracing::error!(%error, task_id = %task.id, "failed to register task ownership");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    } else {
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            task_id = %task.id,
            "set_ownership skipped: instance_pool not attached"
        );
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["tasks"])
            .inc();
    }

    emit_task_event(&state, &task, "created");
    maybe_emit_approval_notification(&state, &task);
    Ok(Json(TaskResponse { task }))
}

/// `PUT /tasks/{number}` — update a task.
#[utoipa::path(
    put,
    path = "/tasks/{number}",
    params(
        ("number" = i64, Path, description = "Task number"),
    ),
    request_body = UpdateTaskRequest,
    responses(
        (status = 200, body = TaskResponse),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Task not found"),
        (status = 503, description = "Task store not initialized"),
    ),
    tag = "tasks",
)]
pub(super) async fn update_task(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Path(number): Path<i64>,
    Json(request): Json<UpdateTaskRequest>,
) -> Result<Json<TaskResponse>, StatusCode> {
    let store = get_task_store(&state)?;

    let status = parse_status(request.status.as_deref())?;
    let priority = parse_priority(request.priority.as_deref())?;

    // Fetch-before-gate: URL path is task_number (i64), ownership keys on
    // task.id (UUID). Missing-task 404 and NotOwned 404 are the same
    // client-visible shape.
    let existing = store
        .get_by_number(number)
        .await
        .map_err(|error| {
            tracing::warn!(%error, task_number = number, "failed to load task for update authz");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let access = crate::auth::check_write(&pool, &auth_ctx, "task", &existing.id)
            .await
            .map_err(|error| {
                tracing::warn!(
                    %error,
                    actor = %auth_ctx.principal_key(),
                    resource_type = "task",
                    resource_id = %existing.id,
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
            .with_label_values(&["tasks"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            task_id = %existing.id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    // Reuse the pre-gate fetch of `existing` — store.update_prefetched
    // skips the duplicate SELECT that store.update would do internally.
    let task = store
        .update_prefetched(
            existing,
            crate::tasks::UpdateTaskInput {
                title: request.title,
                description: request.description,
                status,
                priority,
                assigned_agent_id: request.assigned_agent_id,
                subtasks: request.subtasks,
                metadata: request.metadata,
                worker_id: request.worker_id,
                clear_worker_id: false,
                approved_by: request.approved_by,
                complete_subtask: request.complete_subtask,
            },
        )
        .await
        .map_err(|error| {
            tracing::warn!(%error, task_number = number, "failed to update task");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    emit_task_event(&state, &task, "updated");
    maybe_emit_approval_notification(&state, &task);
    Ok(Json(TaskResponse { task }))
}

/// `DELETE /tasks/{number}` — delete a task.
#[utoipa::path(
    delete,
    path = "/tasks/{number}",
    params(
        ("number" = i64, Path, description = "Task number"),
    ),
    responses(
        (status = 200, body = TaskActionResponse),
        (status = 404, description = "Task not found"),
        (status = 503, description = "Task store not initialized"),
    ),
    tag = "tasks",
)]
pub(super) async fn delete_task(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Path(number): Path<i64>,
) -> Result<Json<TaskActionResponse>, StatusCode> {
    let store = get_task_store(&state)?;

    // Fetch before delete so we can emit an event with the correct agent_id
    // and resolve task.id (UUID) for the authz gate. Missing-task 404 and
    // NotOwned 404 collapse to the same client-visible shape.
    let task = store
        .get_by_number(number)
        .await
        .map_err(|error| {
            tracing::warn!(%error, task_number = number, "failed to get task for deletion");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let access = crate::auth::check_write(&pool, &auth_ctx, "task", &task.id)
            .await
            .map_err(|error| {
                tracing::warn!(
                    %error,
                    actor = %auth_ctx.principal_key(),
                    resource_type = "task",
                    resource_id = %task.id,
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
            .with_label_values(&["tasks"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            task_id = %task.id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let deleted = store.delete(number).await.map_err(|error| {
        tracing::warn!(%error, task_number = number, "failed to delete task");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if !deleted {
        return Err(StatusCode::NOT_FOUND);
    }

    state
        .event_tx
        .send(super::state::ApiEvent::TaskUpdated {
            agent_id: task.assigned_agent_id,
            task_number: number,
            status: "deleted".to_string(),
            action: "deleted".to_string(),
        })
        .ok();

    Ok(Json(TaskActionResponse {
        success: true,
        message: format!("Task #{number} deleted"),
    }))
}

/// `POST /tasks/{number}/approve` — approve a task (move to ready).
#[utoipa::path(
    post,
    path = "/tasks/{number}/approve",
    params(
        ("number" = i64, Path, description = "Task number"),
    ),
    request_body = ApproveRequest,
    responses(
        (status = 200, body = TaskResponse),
        (status = 404, description = "Task not found"),
        (status = 503, description = "Task store not initialized"),
    ),
    tag = "tasks",
)]
pub(super) async fn approve_task(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Path(number): Path<i64>,
    Json(request): Json<ApproveRequest>,
) -> Result<Json<TaskResponse>, StatusCode> {
    let store = get_task_store(&state)?;

    // Fetch-before-gate: URL path is task_number (i64), ownership keys on
    // task.id (UUID). Missing-task 404 and NotOwned 404 are the same
    // client-visible shape.
    let existing = store
        .get_by_number(number)
        .await
        .map_err(|error| {
            tracing::warn!(%error, task_number = number, "failed to load task for approve authz");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let access = crate::auth::check_write(&pool, &auth_ctx, "task", &existing.id)
            .await
            .map_err(|error| {
                tracing::warn!(
                    %error,
                    actor = %auth_ctx.principal_key(),
                    resource_type = "task",
                    resource_id = %existing.id,
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
            .with_label_values(&["tasks"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            task_id = %existing.id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    // Reuse the pre-gate fetch of `existing` — store.update_prefetched
    // skips the duplicate SELECT that store.update would do internally.
    let task = store
        .update_prefetched(
            existing,
            crate::tasks::UpdateTaskInput {
                status: Some(crate::tasks::TaskStatus::Ready),
                approved_by: request.approved_by,
                ..Default::default()
            },
        )
        .await
        .map_err(|error| {
            tracing::warn!(%error, task_number = number, "failed to approve task");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    emit_task_event(&state, &task, "updated");
    // Auto-dismiss any pending task_approval notification for this task.
    if let Some(store) = state.notification_store.load().as_ref().clone()
        && let Err(error) = store
            .dismiss_by_entity("task_approval", "task", &number.to_string())
            .await
    {
        tracing::warn!(%error, task_number = number, "failed to auto-dismiss approval notification");
    }
    Ok(Json(TaskResponse { task }))
}

/// `POST /tasks/{number}/execute` — move a task to ready for execution.
/// Tasks already in `ready` or `in_progress` are returned as-is.
#[utoipa::path(
    post,
    path = "/tasks/{number}/execute",
    params(
        ("number" = i64, Path, description = "Task number"),
    ),
    request_body = ApproveRequest,
    responses(
        (status = 200, body = TaskResponse),
        (status = 404, description = "Task not found"),
        (status = 409, description = "Task pending approval"),
        (status = 503, description = "Task store not initialized"),
    ),
    tag = "tasks",
)]
pub(super) async fn execute_task(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Path(number): Path<i64>,
    Json(request): Json<ApproveRequest>,
) -> Result<Json<TaskResponse>, StatusCode> {
    let store = get_task_store(&state)?;

    let current = store
        .get_by_number(number)
        .await
        .map_err(|error| {
            tracing::warn!(%error, task_number = number, "failed to get task for execution");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Gate on the already-fetched task.id (UUID); URL path carries only
    // task_number. Missing-task 404 and NotOwned 404 collapse.
    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let access = crate::auth::check_write(&pool, &auth_ctx, "task", &current.id)
            .await
            .map_err(|error| {
                tracing::warn!(
                    %error,
                    actor = %auth_ctx.principal_key(),
                    resource_type = "task",
                    resource_id = %current.id,
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
            .with_label_values(&["tasks"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            task_id = %current.id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    if matches!(
        current.status,
        crate::tasks::TaskStatus::Ready | crate::tasks::TaskStatus::InProgress
    ) {
        return Ok(Json(TaskResponse { task: current }));
    }

    // Reject pending_approval tasks: they must be approved first.
    if current.status == crate::tasks::TaskStatus::PendingApproval {
        return Err(StatusCode::CONFLICT);
    }

    // Reuse the pre-gate fetch of `current` — store.update_prefetched
    // skips the duplicate SELECT that store.update would do internally.
    let task = store
        .update_prefetched(
            current,
            crate::tasks::UpdateTaskInput {
                status: Some(crate::tasks::TaskStatus::Ready),
                approved_by: request.approved_by,
                ..Default::default()
            },
        )
        .await
        .map_err(|error| {
            tracing::warn!(%error, task_number = number, "failed to execute task");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    emit_task_event(&state, &task, "updated");
    Ok(Json(TaskResponse { task }))
}

/// `POST /tasks/{number}/assign` — reassign a task to a different agent.
#[utoipa::path(
    post,
    path = "/tasks/{number}/assign",
    params(
        ("number" = i64, Path, description = "Task number"),
    ),
    request_body = AssignRequest,
    responses(
        (status = 200, body = TaskResponse),
        (status = 404, description = "Task not found"),
        (status = 503, description = "Task store not initialized"),
    ),
    tag = "tasks",
)]
pub(super) async fn assign_task(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Path(number): Path<i64>,
    Json(request): Json<AssignRequest>,
) -> Result<Json<TaskResponse>, StatusCode> {
    let store = get_task_store(&state)?;

    // Fetch-before-gate: URL path is task_number (i64), ownership keys on
    // task.id (UUID). Missing-task 404 and NotOwned 404 are the same
    // client-visible shape.
    let existing = store
        .get_by_number(number)
        .await
        .map_err(|error| {
            tracing::warn!(%error, task_number = number, "failed to load task for assign authz");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let access = crate::auth::check_write(&pool, &auth_ctx, "task", &existing.id)
            .await
            .map_err(|error| {
                tracing::warn!(
                    %error,
                    actor = %auth_ctx.principal_key(),
                    resource_type = "task",
                    resource_id = %existing.id,
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
            .with_label_values(&["tasks"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            task_id = %existing.id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    // Reuse the pre-gate fetch of `existing` — store.update_prefetched
    // skips the duplicate SELECT that store.update would do internally.
    let task = store
        .update_prefetched(
            existing,
            crate::tasks::UpdateTaskInput {
                assigned_agent_id: Some(request.assigned_agent_id),
                ..Default::default()
            },
        )
        .await
        .map_err(|error| {
            tracing::warn!(%error, task_number = number, "failed to assign task");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    emit_task_event(&state, &task, "updated");
    Ok(Json(TaskResponse { task }))
}
