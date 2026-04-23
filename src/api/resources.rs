//! `PUT /api/resources/{resource_type}/{resource_id}/visibility` — rotate a
//! resource's visibility between Personal / Team / Org and re-bind the
//! optional `shared_with_team_id`. Phase 7 PR 1.5 Task 7.5.
//!
//! Semantics:
//! - `check_write` gates: owner OR admin may change visibility. Non-owner
//!   non-admin gets 404 per the no-auto-broadening policy so a stranger
//!   cannot even confirm the resource exists.
//! - The handler validates the payload (visibility parse + team-without-
//!   team-id) BEFORE touching the pool, so malformed requests surface as
//!   400 Bad Request rather than 500 Internal Server Error from a CHECK
//!   constraint violation.
//! - On success, `set_ownership` upserts the ownership row (new
//!   `visibility` + `shared_with_team_id`; `owner_principal_key` is
//!   preserved as the caller, which matches the Phase 2 ownership model
//!   where the caller is the authoritative owner at write time).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use std::sync::Arc;

use crate::api::state::ApiState;
use crate::auth::context::AuthContext;
use crate::auth::policy::check_write;
use crate::auth::principals::Visibility;
use crate::auth::repository::set_ownership;

/// Payload accepted by `PUT /api/resources/{type}/{id}/visibility`. Keep the
/// wire shape snake_case (Rust default) so the TS client can pass
/// `{visibility, shared_with_team_id}` without custom serde rules.
#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct SetVisibilityRequest {
    visibility: String,
    #[serde(default)]
    shared_with_team_id: Option<String>,
}

#[utoipa::path(
    put,
    path = "/resources/{resource_type}/{resource_id}/visibility",
    params(
        ("resource_type" = String, Path, description = "Resource type (memory, task, wiki, cron, portal, agent, etc.)"),
        ("resource_id" = String, Path, description = "Resource identifier"),
    ),
    request_body = SetVisibilityRequest,
    responses(
        (status = 200, description = "Visibility updated"),
        (status = 400, description = "Invalid visibility value or missing team_id for team scope"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Authenticated but not authorized"),
        (status = 404, description = "Resource not found or caller is not owner/admin"),
    ),
    tag = "resources",
)]
pub(super) async fn set_visibility(
    State(state): State<Arc<ApiState>>,
    auth_ctx: AuthContext,
    Path((resource_type, resource_id)): Path<(String, String)>,
    Json(req): Json<SetVisibilityRequest>,
) -> Result<StatusCode, StatusCode> {
    // Parse + guard BEFORE touching the pool so malformed requests fail
    // fast with a clear 400 (not a 500 CHECK-constraint leak from the DB
    // layer). The Visibility CHECK on the `resource_ownership` table
    // enforces the same invariant as a belt-and-suspenders defense.
    let vis = Visibility::parse(&req.visibility).ok_or(StatusCode::BAD_REQUEST)?;
    if matches!(vis, Visibility::Team) && req.shared_with_team_id.is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // D30 correction: canonical ArcSwap peek matching `src/api/me.rs:60`.
    let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() else {
        tracing::warn!(
            actor = %auth_ctx.principal_key(),
            resource_type = %resource_type,
            resource_id = %resource_id,
            "set_visibility: instance_pool not attached (boot window or startup-ordering bug)"
        );
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    };

    let access = check_write(&pool, &auth_ctx, &resource_type, &resource_id)
        .await
        .map_err(|error| {
            tracing::warn!(
                %error,
                actor = %auth_ctx.principal_key(),
                resource_type = %resource_type,
                resource_id = %resource_id,
                "authz check_write failed"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if !access.is_allowed() {
        return Err(access.to_status());
    }

    set_ownership(
        &pool,
        &resource_type,
        &resource_id,
        None,
        &auth_ctx.principal_key(),
        vis,
        req.shared_with_team_id.as_deref(),
    )
    .await
    .map_err(|error| {
        tracing::warn!(
            %error,
            actor = %auth_ctx.principal_key(),
            resource_type = %resource_type,
            resource_id = %resource_id,
            "set_visibility: set_ownership failed"
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(StatusCode::OK)
}
