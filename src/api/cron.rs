//! Cron HTTP handlers + their shared Phase-4 authz gate.
//!
//! Six endpoints (`list_cron_jobs`, `cron_executions`, `create_or_update_cron`,
//! `delete_cron`, `trigger_cron`, `toggle_cron`) consult the Phase-4 authz
//! helpers before touching the per-agent cron store or the in-process
//! scheduler. Read handlers call `check_read_with_audit("agent", &agent_id)`
//! because list-style cron reads are agent-scoped (the URL carries
//! `agent_id`; the cron_id narrows within the agent). Write handlers call
//! `check_write("cron_job", &cron_id)` — the write surface keys on the
//! individual cron row per A-09.
//!
//! `create_or_update_cron` is a true upsert at the SQL layer; the handler
//! discriminates new-vs-existing via `store.load(&cron_id)`. The existing
//! path calls `check_write` before the save; the new path awaits
//! `set_ownership` AFTER the save (A-12: a fire-and-forget `tokio::spawn`
//! would race the creator's subsequent GET into a 404).
//!
//! Scheduled cron runs execute as `PrincipalType::System`. `is_admin`
//! includes `System` in its bypass set (see `src/auth/roles.rs`), so
//! `check_read` and `check_write` allow those principals without reaching
//! the ownership table. The
//! `system_can_read_cron_of_disabled_user` regression test in
//! `tests/api_cron_authz.rs` guards against a future narrowing of
//! `is_admin` that would silently break scheduled execution for disabled
//! or deleted user-owned crons.
//!
//! The ~45-line gate block is **inlined at each call site on purpose**
//! (Phase 4 PR 2 decision N1 in
//! `.scratchpad/plans/entraid-auth/phase-4-authz-helpers.md`). A helper
//! would save writing but hurt grep-by-handler visibility during route
//! review. The metric label is always `"cron"` (file resource family),
//! never a per-handler sub-label, to keep
//! `spacebot_authz_skipped_total` cardinality flat. Pool-None is a
//! boot-window signal (always-on `tracing::error!` + feature-gated counter
//! increment); a persistent non-zero rate after startup is a
//! startup-ordering regression.
//!
//! Phase 5 replaces the `tracing::info!` admin-override path with an
//! `AuditAppender::append` call against the hash-chained audit log.

use super::state::ApiState;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;

/// Resource-type key for cron ownership rows. Shared across
/// `set_ownership` at the create path, `check_write` at mutate paths,
/// and `enrich_visibility_tags` at the list handler. Extracting the
/// string to a single constant prevents the BUG-C1 class of regression
/// where the enrichment call is keyed on one resource family (e.g.
/// `"cron"`, the metric-label namespace) while the ownership row was
/// written under another (`"cron_job"`, the write-authz namespace);
/// the SQL WHERE clause matches zero rows and chip fields silently
/// render as `None` across the entire surface.
const CRON_RESOURCE_TYPE: &str = "cron_job";

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub(super) struct CronQuery {
    agent_id: String,
}

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub(super) struct CronExecutionsQuery {
    agent_id: String,
    #[serde(default)]
    cron_id: Option<String>,
    #[serde(default = "default_cron_executions_limit")]
    limit: i64,
}

fn default_cron_executions_limit() -> i64 {
    50
}

#[derive(Deserialize, Debug, utoipa::ToSchema)]
pub(super) struct CreateCronRequest {
    agent_id: String,
    id: String,
    prompt: String,
    #[serde(default)]
    cron_expr: Option<String>,
    #[serde(default = "default_interval")]
    interval_secs: u64,
    delivery_target: String,
    #[serde(default)]
    active_start_hour: Option<u8>,
    #[serde(default)]
    active_end_hour: Option<u8>,
    #[serde(default = "default_enabled")]
    enabled: bool,
    #[serde(default)]
    run_once: bool,
    #[serde(default)]
    timeout_secs: Option<u64>,
}

fn default_interval() -> u64 {
    3600
}

fn default_enabled() -> bool {
    true
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct DeleteCronRequest {
    agent_id: String,
    cron_id: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct TriggerCronRequest {
    agent_id: String,
    cron_id: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct ToggleCronRequest {
    agent_id: String,
    cron_id: String,
    enabled: bool,
}

#[derive(Serialize, utoipa::ToSchema)]
struct CronJobWithStats {
    id: String,
    prompt: String,
    cron_expr: Option<String>,
    interval_secs: u64,
    delivery_target: String,
    enabled: bool,
    run_once: bool,
    active_hours: Option<(u8, u8)>,
    timeout_secs: Option<u64>,
    execution_success_count: u64,
    execution_failure_count: u64,
    delivery_success_count: u64,
    delivery_failure_count: u64,
    delivery_skipped_count: u64,
    last_executed_at: Option<String>,
}

/// Cron list row: the bare job shape plus a `VisibilityTag` flattened
/// into the same JSON object. Additive on the wire (clients that ignore
/// unknown fields continue to work; chip-aware clients see the tag).
/// Mirrors `MemoryListItem` / `TaskListItem` / `WikiListItem`.
#[derive(Serialize, utoipa::ToSchema)]
struct CronListItem {
    #[serde(flatten)]
    job: CronJobWithStats,
    #[serde(flatten)]
    tag: crate::api::resources::VisibilityTag,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct CronListResponse {
    jobs: Vec<CronListItem>,
    timezone: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct CronExecutionsResponse {
    executions: Vec<crate::cron::CronExecutionEntry>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct CronActionResponse {
    success: bool,
    message: String,
}

/// List all cron jobs for an agent with execution statistics.
#[utoipa::path(
    get,
    path = "/agents/cron",
    params(
        ("agent_id" = String, Query, description = "Agent ID"),
    ),
    responses(
        (status = 200, body = CronListResponse),
        (status = 404, description = "Agent not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "cron",
)]
pub(super) async fn list_cron_jobs(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Query(query): Query<CronQuery>,
) -> Result<Json<CronListResponse>, StatusCode> {
    // Phase 4 authz gate: cron listing rides the agent's ownership row
    // (mirrors `list_memories` and `list_tasks`). `CronQuery.agent_id` is
    // required, so every call has an agent-scoped filter. There is no
    // separate "no-filter" path like `list_tasks` has to handle.
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
            .with_label_values(&["cron"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            agent_id = %query.agent_id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let stores = state.cron_stores.load();
    let schedulers = state.cron_schedulers.load();
    let store = stores.get(&query.agent_id).ok_or(StatusCode::NOT_FOUND)?;
    let scheduler = schedulers
        .get(&query.agent_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    let configs = store.load_all_unfiltered().await.map_err(|error| {
        tracing::warn!(%error, agent_id = %query.agent_id, "failed to load cron jobs");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Batch-enrich visibility + team_name for the whole page in one
    // roundtrip against the instance pool. Cron lives in a per-agent
    // CronStore (cron_jobs.db per agent) while resource_ownership + teams
    // live in the instance pool, and SQLite does not support
    // cross-database JOIN; see CRON_RESOURCE_TYPE for the keying
    // invariant shared across set / check / enrich.
    let ids: Vec<String> = configs.iter().map(|c| c.id.clone()).collect();
    let tags = if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        crate::api::resources::enrich_visibility_tags(&pool, CRON_RESOURCE_TYPE, &ids).await
    } else {
        // I4: mirror the authz-skipped pattern.
        tracing::warn!(
            handler = "cron",
            count = ids.len(),
            "enrichment skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
        std::collections::HashMap::new()
    };

    let mut jobs = Vec::new();
    for config in configs {
        let stats = store
            .get_execution_stats(&config.id)
            .await
            .unwrap_or_default();
        let tag = tags.get(&config.id).cloned().unwrap_or_default();
        let job = CronJobWithStats {
            id: config.id,
            prompt: config.prompt,
            cron_expr: config.cron_expr,
            interval_secs: config.interval_secs,
            delivery_target: config.delivery_target,
            enabled: config.enabled,
            run_once: config.run_once,
            active_hours: config.active_hours,
            timeout_secs: config.timeout_secs,
            execution_success_count: stats.execution_success_count,
            execution_failure_count: stats.execution_failure_count,
            delivery_success_count: stats.delivery_success_count,
            delivery_failure_count: stats.delivery_failure_count,
            delivery_skipped_count: stats.delivery_skipped_count,
            last_executed_at: stats.last_executed_at,
        };
        jobs.push(CronListItem { job, tag });
    }

    Ok(Json(CronListResponse {
        jobs,
        timezone: scheduler.cron_timezone_label(),
    }))
}

/// Get execution history for cron jobs.
#[utoipa::path(
    get,
    path = "/agents/cron/executions",
    params(
        ("agent_id" = String, Query, description = "Agent ID"),
        ("cron_id" = Option<String>, Query, description = "Cron job ID (optional)"),
        ("limit" = i64, Query, description = "Maximum number of executions to return (default 50)"),
    ),
    responses(
        (status = 200, body = CronExecutionsResponse),
        (status = 404, description = "Agent not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "cron",
)]
pub(super) async fn cron_executions(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Query(query): Query<CronExecutionsQuery>,
) -> Result<Json<CronExecutionsResponse>, StatusCode> {
    // Phase 4 authz gate: execution history is agent-scoped. `agent_id` is
    // required on the query; `cron_id` narrows within the agent. Gate on
    // the agent resource (mirrors `list_cron_jobs`).
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
            .with_label_values(&["cron"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            agent_id = %query.agent_id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let stores = state.cron_stores.load();
    let store = stores.get(&query.agent_id).ok_or(StatusCode::NOT_FOUND)?;

    let executions = if let Some(cron_id) = query.cron_id {
        store
            .load_executions(&cron_id, query.limit)
            .await
            .map_err(|error| {
                tracing::warn!(%error, agent_id = %query.agent_id, cron_id = %cron_id, "failed to load cron executions");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
    } else {
        store
            .load_all_executions(query.limit)
            .await
            .map_err(|error| {
                tracing::warn!(%error, agent_id = %query.agent_id, "failed to load cron executions");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
    };

    Ok(Json(CronExecutionsResponse { executions }))
}

const MIN_CRON_INTERVAL_SECS: u64 = 60;
const MAX_CRON_PROMPT_LENGTH: usize = 10_000;

fn validate_cron_request(request: &CreateCronRequest) -> Result<(), (StatusCode, String)> {
    if request.id.is_empty()
        || request.id.len() > 50
        || !request
            .id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "id must be 1-50 alphanumeric/hyphen/underscore characters".into(),
        ));
    }

    let cron_expr = request
        .cron_expr
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    if cron_expr.is_none() && request.interval_secs < MIN_CRON_INTERVAL_SECS {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "interval_secs must be at least {MIN_CRON_INTERVAL_SECS} (got {})",
                request.interval_secs
            ),
        ));
    }

    if let Some(expr) = cron_expr {
        let field_count = expr.split_whitespace().count();
        if field_count != 5 {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("cron_expr must have exactly 5 fields (got {field_count}): '{expr}'"),
            ));
        }
        // The `cron` crate uses 7-field expressions (sec min hour dom month dow year).
        // Users write standard 5-field cron (min hour dom month dow). Expand before parsing.
        let expanded = format!("0 {expr} *");
        cron::Schedule::from_str(&expanded).map_err(|error| {
            (
                StatusCode::BAD_REQUEST,
                format!("invalid cron_expr '{expr}': {error}"),
            )
        })?;
    }

    if request.prompt.len() > MAX_CRON_PROMPT_LENGTH {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "prompt exceeds maximum length of {MAX_CRON_PROMPT_LENGTH} characters (got {})",
                request.prompt.len()
            ),
        ));
    }

    if !request.delivery_target.contains(':') {
        return Err((
            StatusCode::BAD_REQUEST,
            "delivery_target must be in 'adapter:target' format".into(),
        ));
    }

    if let Some(start) = request.active_start_hour
        && start > 23
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "active_start_hour must be 0-23".into(),
        ));
    }
    if let Some(end) = request.active_end_hour
        && end > 23
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "active_end_hour must be 0-23".into(),
        ));
    }

    Ok(())
}

/// Create or update a cron job.
#[utoipa::path(
    post,
    path = "/agents/cron",
    request_body = CreateCronRequest,
    responses(
        (status = 200, body = CronActionResponse),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Agent not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "cron",
)]
pub(super) async fn create_or_update_cron(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Json(request): Json<CreateCronRequest>,
) -> Result<Json<CronActionResponse>, (StatusCode, Json<CronActionResponse>)> {
    if let Err((status, message)) = validate_cron_request(&request) {
        tracing::warn!(agent_id = %request.agent_id, cron_id = %request.id, %message, "cron validation failed");
        return Err((
            status,
            Json(CronActionResponse {
                success: false,
                message,
            }),
        ));
    }

    let stores = state.cron_stores.load();
    let schedulers = state.cron_schedulers.load();

    let cron_err = |status: StatusCode, message: String| {
        (
            status,
            Json(CronActionResponse {
                success: false,
                message,
            }),
        )
    };

    let store = stores.get(&request.agent_id).ok_or_else(|| {
        cron_err(
            StatusCode::NOT_FOUND,
            format!("agent '{}' not found", request.agent_id),
        )
    })?;
    let scheduler = schedulers.get(&request.agent_id).ok_or_else(|| {
        cron_err(
            StatusCode::NOT_FOUND,
            format!("agent '{}' not found", request.agent_id),
        )
    })?;

    // Phase 4 authz gate: branch on new-vs-existing cron. If the cron
    // already exists, this is an update: gate via `check_write` against
    // the existing ownership row. If it does not exist, this is a create —
    // no pre-existing row to gate on; after a successful `store.save` the
    // handler awaits `set_ownership` so the creator's subsequent reads
    // pass the gate (A-12: fire-and-forget `tokio::spawn` would race a
    // GET into 404).
    let existing = store.load(&request.id).await.map_err(|error| {
        tracing::warn!(%error, agent_id = %request.agent_id, cron_id = %request.id, "failed to load cron for authz discriminate");
        cron_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load cron: {error}"),
        )
    })?;
    let is_new = existing.is_none();

    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        if !is_new {
            let access =
                crate::auth::check_write(&pool, &auth_ctx, CRON_RESOURCE_TYPE, &request.id)
                    .await
                    .map_err(|error| {
                        tracing::warn!(
                            %error,
                            actor = %auth_ctx.principal_key(),
                            resource_type = CRON_RESOURCE_TYPE,
                            resource_id = %request.id,
                            "authz check_write failed"
                        );
                        cron_err(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "authz check failed".to_string(),
                        )
                    })?;
            if !access.is_allowed() {
                crate::auth::policy::fire_denied_audit(
                    &state.audit,
                    &auth_ctx,
                    CRON_RESOURCE_TYPE,
                    request.id.as_str(),
                );
                return Err(cron_err(
                    access.to_status(),
                    format!("cron '{}' not accessible", request.id),
                ));
            }
        }
    } else {
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["cron"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            cron_id = %request.id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let active_hours = match (request.active_start_hour, request.active_end_hour) {
        (Some(start), Some(end)) => Some((start, end)),
        _ => None,
    };

    let config = crate::cron::CronConfig {
        id: request.id.clone(),
        prompt: request.prompt,
        cron_expr: request
            .cron_expr
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
        interval_secs: request.interval_secs,
        delivery_target: request.delivery_target,
        active_hours,
        enabled: request.enabled,
        run_once: request.run_once,
        next_run_at: None,
        timeout_secs: request.timeout_secs,
    };

    store.save(&config).await.map_err(|error| {
        tracing::warn!(%error, agent_id = %request.agent_id, cron_id = %request.id, "failed to save cron job");
        cron_err(StatusCode::INTERNAL_SERVER_ERROR, format!("failed to save: {error}"))
    })?;

    // A-12: new-cron path awaits `set_ownership` BEFORE returning AND
    // before `scheduler.register` so a scheduler.register failure does
    // not leave the SQL row with no owner.
    if is_new {
        if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
            crate::auth::repository::set_ownership(
                &pool,
                CRON_RESOURCE_TYPE,
                &request.id,
                Some(&request.agent_id),
                &auth_ctx.principal_key(),
                crate::auth::principals::Visibility::Personal,
                None,
            )
            .await
            .map_err(|error| {
                tracing::error!(%error, cron_id = %request.id, "failed to register cron ownership");
                cron_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "failed to register ownership".to_string(),
                )
            })?;
        } else {
            tracing::error!(
                actor = %auth_ctx.principal_key(),
                cron_id = %request.id,
                "set_ownership skipped: instance_pool not attached"
            );
            #[cfg(feature = "metrics")]
            crate::telemetry::Metrics::global()
                .authz_skipped_total
                .with_label_values(&["cron"])
                .inc();
        }
    }

    scheduler.register(config).await.map_err(|error| {
        tracing::warn!(%error, agent_id = %request.agent_id, cron_id = %request.id, "failed to register cron job");
        cron_err(StatusCode::INTERNAL_SERVER_ERROR, format!("failed to register: {error}"))
    })?;

    Ok(Json(CronActionResponse {
        success: true,
        message: format!("Cron job '{}' saved successfully", request.id),
    }))
}

/// Delete a cron job.
#[utoipa::path(
    delete,
    path = "/agents/cron",
    params(
        ("agent_id" = String, Query, description = "Agent ID"),
        ("cron_id" = String, Query, description = "Cron job ID to delete"),
    ),
    responses(
        (status = 200, body = CronActionResponse),
        (status = 404, description = "Agent not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "cron",
)]
pub(super) async fn delete_cron(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Query(query): Query<DeleteCronRequest>,
) -> Result<Json<CronActionResponse>, StatusCode> {
    // Phase 4 authz gate: per-cron write. NotOwned/NotYours both collapse
    // to 404 (hide existence).
    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let access = crate::auth::check_write(&pool, &auth_ctx, CRON_RESOURCE_TYPE, &query.cron_id)
            .await
            .map_err(|error| {
                tracing::warn!(
                    %error,
                    actor = %auth_ctx.principal_key(),
                    resource_type = CRON_RESOURCE_TYPE,
                    resource_id = %query.cron_id,
                    "authz check_write failed"
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if !access.is_allowed() {
            crate::auth::policy::fire_denied_audit(
                &state.audit,
                &auth_ctx,
                CRON_RESOURCE_TYPE,
                query.cron_id.as_str(),
            );
            return Err(access.to_status());
        }
    } else {
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["cron"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            cron_id = %query.cron_id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let stores = state.cron_stores.load();
    let store = stores.get(&query.agent_id).ok_or(StatusCode::NOT_FOUND)?;

    let schedulers = state.cron_schedulers.load();
    let scheduler = schedulers
        .get(&query.agent_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    scheduler.unregister(&query.cron_id).await;

    store.delete(&query.cron_id).await.map_err(|error| {
        tracing::warn!(%error, agent_id = %query.agent_id, cron_id = %query.cron_id, "failed to delete cron job");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(CronActionResponse {
        success: true,
        message: format!("Cron job '{}' deleted successfully", query.cron_id),
    }))
}

/// Trigger a cron job immediately.
#[utoipa::path(
    post,
    path = "/agents/cron/trigger",
    request_body = TriggerCronRequest,
    responses(
        (status = 200, body = CronActionResponse),
        (status = 404, description = "Agent not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "cron",
)]
pub(super) async fn trigger_cron(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Json(request): Json<TriggerCronRequest>,
) -> Result<Json<CronActionResponse>, StatusCode> {
    // Phase 4 authz gate: triggering a cron is a write action on the cron
    // row (it mutates the scheduler's in-memory job state and causes a
    // side-effecting run).
    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let access =
            crate::auth::check_write(&pool, &auth_ctx, CRON_RESOURCE_TYPE, &request.cron_id)
                .await
                .map_err(|error| {
                    tracing::warn!(
                        %error,
                        actor = %auth_ctx.principal_key(),
                        resource_type = CRON_RESOURCE_TYPE,
                        resource_id = %request.cron_id,
                        "authz check_write failed"
                    );
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
        if !access.is_allowed() {
            crate::auth::policy::fire_denied_audit(
                &state.audit,
                &auth_ctx,
                CRON_RESOURCE_TYPE,
                request.cron_id.as_str(),
            );
            return Err(access.to_status());
        }
    } else {
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["cron"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            cron_id = %request.cron_id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let schedulers = state.cron_schedulers.load();
    let scheduler = schedulers
        .get(&request.agent_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    scheduler.trigger_now(&request.cron_id).await.map_err(|error| {
        tracing::warn!(%error, agent_id = %request.agent_id, cron_id = %request.cron_id, "failed to trigger cron job");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(CronActionResponse {
        success: true,
        message: format!("Cron job '{}' triggered", request.cron_id),
    }))
}

/// Enable or disable a cron job.
#[utoipa::path(
    put,
    path = "/agents/cron/toggle",
    request_body = ToggleCronRequest,
    responses(
        (status = 200, body = CronActionResponse),
        (status = 404, description = "Agent not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "cron",
)]
pub(super) async fn toggle_cron(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Json(request): Json<ToggleCronRequest>,
) -> Result<Json<CronActionResponse>, StatusCode> {
    // Phase 4 authz gate: enable/disable mutates cron state.
    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let access =
            crate::auth::check_write(&pool, &auth_ctx, CRON_RESOURCE_TYPE, &request.cron_id)
                .await
                .map_err(|error| {
                    tracing::warn!(
                        %error,
                        actor = %auth_ctx.principal_key(),
                        resource_type = CRON_RESOURCE_TYPE,
                        resource_id = %request.cron_id,
                        "authz check_write failed"
                    );
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
        if !access.is_allowed() {
            crate::auth::policy::fire_denied_audit(
                &state.audit,
                &auth_ctx,
                CRON_RESOURCE_TYPE,
                request.cron_id.as_str(),
            );
            return Err(access.to_status());
        }
    } else {
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["cron"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            cron_id = %request.cron_id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let stores = state.cron_stores.load();
    let store = stores.get(&request.agent_id).ok_or(StatusCode::NOT_FOUND)?;

    let schedulers = state.cron_schedulers.load();
    let scheduler = schedulers
        .get(&request.agent_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    store.update_enabled(&request.cron_id, request.enabled).await.map_err(|error| {
        tracing::warn!(%error, agent_id = %request.agent_id, cron_id = %request.cron_id, "failed to update cron job enabled state");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    scheduler.set_enabled(&request.cron_id, request.enabled).await.map_err(|error| {
        tracing::warn!(%error, agent_id = %request.agent_id, cron_id = %request.cron_id, "failed to update scheduler enabled state");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let status = if request.enabled {
        "enabled"
    } else {
        "disabled"
    };
    Ok(Json(CronActionResponse {
        success: true,
        message: format!("Cron job '{}' {}", request.cron_id, status),
    }))
}

#[cfg(test)]
mod tests {
    use super::{CronJobWithStats, CronListItem};
    use crate::api::resources::VisibilityTag;
    use crate::auth::principals::Visibility;

    fn sample_job(id: &str) -> CronJobWithStats {
        CronJobWithStats {
            id: id.into(),
            prompt: "do the thing".into(),
            cron_expr: None,
            interval_secs: 60,
            delivery_target: "bulletin".into(),
            enabled: true,
            run_once: false,
            active_hours: None,
            timeout_secs: None,
            execution_success_count: 0,
            execution_failure_count: 0,
            delivery_success_count: 0,
            delivery_failure_count: 0,
            delivery_skipped_count: 0,
            last_executed_at: None,
        }
    }

    /// Pin the wire shape for the flattened chip fields. CronListItem
    /// uses `#[serde(flatten)]` on both the inner `CronJobWithStats`
    /// and the `VisibilityTag`; an accidental rewrap (nested
    /// `visibility: { ... }`, or dropped `flatten`) would break the
    /// SPA's VisibilityChip consumer. This test freezes the
    /// skip_serializing_if contract for the `None` case.
    #[test]
    fn visibility_fields_omitted_when_none() {
        let item = CronListItem {
            job: sample_job("c-1"),
            tag: VisibilityTag::default(),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(
            !json.contains("\"visibility\""),
            "visibility: None must be omitted from wire: {json}"
        );
        assert!(
            !json.contains("\"team_name\""),
            "team_name: None must be omitted from wire: {json}"
        );
    }

    #[test]
    fn visibility_fields_present_when_some() {
        let item = CronListItem {
            job: sample_job("c-2"),
            tag: VisibilityTag::new(Some(Visibility::Team), Some("Platform".into())),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"visibility\":\"team\""));
        assert!(json.contains("\"team_name\":\"Platform\""));
        // Flat shape guard: the inner job's fields must appear at the
        // top level of the JSON object, not nested under `job`.
        assert!(
            !json.contains("\"job\":"),
            "job must be flattened, not nested: {json}"
        );
        assert!(json.contains("\"id\":\"c-2\""));
    }

    /// Serialized `CronListItem` must expose the union of `CronJobWithStats`
    /// and `VisibilityTag` fields with no overwrites. `#[serde(flatten)]` on
    /// both members silently drops a key when names collide, which would
    /// mask a future `VisibilityTag` field rename or a new
    /// `CronJobWithStats` field whose name happens to match an existing tag
    /// field. Mirrors `project_list_item_flatten_has_no_key_collision` in
    /// `src/api/projects.rs`.
    #[test]
    fn cron_list_item_flatten_has_no_key_collision() {
        let job = sample_job("c-3");
        let tag = VisibilityTag::new(Some(Visibility::Team), Some("Platform".into()));
        let item = CronListItem {
            job: sample_job("c-3"),
            tag,
        };
        let wrapper = serde_json::to_value(&item).expect("serialize CronListItem");
        let wrapper_keys: Vec<String> = wrapper
            .as_object()
            .expect("top-level object")
            .keys()
            .cloned()
            .collect();
        let job_keys: Vec<String> = serde_json::to_value(&job)
            .expect("serialize CronJobWithStats")
            .as_object()
            .expect("job object")
            .keys()
            .cloned()
            .collect();
        for key in &job_keys {
            assert!(
                wrapper_keys.contains(key),
                "CronJobWithStats field `{key}` was dropped by #[serde(flatten)] \
                 collision with VisibilityTag; wrapper keys: {wrapper_keys:?}"
            );
        }
        for tag_key in ["visibility", "team_name"] {
            assert!(
                !job_keys.iter().any(|k| k == tag_key),
                "name collision: `{tag_key}` exists on both CronJobWithStats and \
                 VisibilityTag; #[serde(flatten)] would silently drop one."
            );
        }
        assert_eq!(
            wrapper_keys.len(),
            job_keys.len() + 2,
            "wrapper key count should be CronJobWithStats fields + 2 VisibilityTag \
             fields; got {} expected {}. Keys: {:?}",
            wrapper_keys.len(),
            job_keys.len() + 2,
            wrapper_keys
        );
    }
}
