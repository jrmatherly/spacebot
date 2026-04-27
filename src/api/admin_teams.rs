//! Admin-only team directory endpoints.
//!
//! Two routes back the `/admin/teams` SPA page:
//!
//! - `GET /admin/teams` returns the active team roster with aggregated
//!   member counts and the most-recent membership observation timestamp
//!   per team.
//! - `GET /admin/teams/{id}/members` returns the trimmed user roster for
//!   a single team.
//!
//! Both routes gate on `ROLE_ADMIN` and return 403 to any other caller,
//! mirroring the `audit::list_audit_events` precedent (Phase 5). Response
//! DTOs are deliberately narrower than the underlying `TeamRecord` /
//! `UserRecord` rows: the admin UI surfaces display names and counts, not
//! tenant ids or external object ids, so the trimmed DTOs keep the wire
//! shape stable even if the record shape grows.
//!
//! The "Sync from Graph" button deferred to a later PR per C1 decision:
//! a visible button wired to a no-op endpoint is a silent failure, so
//! the button and its backend helper land together.

use super::state::ApiState;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Serialize;
use std::sync::Arc;

use crate::auth::context::AuthContext;
use crate::auth::roles::{ROLE_ADMIN, require_role};

/// Persisted team lifecycle state. Mirrors the CHECK constraint on
/// `teams.status` (`CHECK (status IN ('active', 'archived'))`) from
/// `migrations/global/20260420120002_teams.sql`. Typed here so the admin
/// wire response can't accidentally emit a string that doesn't match the
/// schema. `list_admin_teams` filters to `Active` at the SQL layer today,
/// but the `Archived` variant is present so a future "show archived
/// teams" admin toggle doesn't require a wire-shape migration.
#[derive(Debug, Clone, Copy, Serialize, utoipa::ToSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(rename_all = "lowercase")]
pub(super) enum TeamStatus {
    Active,
    Archived,
}

/// SQL projection row for `list_admin_teams`. Local to this module because
/// the shape is the query's output, not a broader domain type. Named
/// fields (vs a tuple) keep the `list_admin_teams` body readable and
/// sidestep the `clippy::type_complexity` warning a 5-tuple would trigger.
#[derive(sqlx::FromRow)]
struct AdminTeamRow {
    id: String,
    display_name: String,
    status: TeamStatus,
    member_count: i64,
    last_sync_at: Option<String>,
}

/// SQL projection row for `list_team_members`. Same locality rationale as
/// [`AdminTeamRow`]: the shape is query-specific, not part of any domain
/// type, and a named struct beats a 5-tuple under clippy.
#[derive(sqlx::FromRow)]
struct AdminTeamMemberRow {
    principal_key: String,
    display_name: Option<String>,
    display_email: Option<String>,
    observed_at: String,
    source: String,
}

/// List row for `GET /admin/teams`. Trimmed versus [`crate::auth::principals::TeamRecord`]
/// so the wire response does not expose `external_id` (Entra group id) or
/// raw `created_at` / `updated_at` timestamps that the admin UI does not
/// surface. `last_sync_at` maps to `MAX(team_memberships.observed_at)` so
/// the UI can show "data is N hours stale" at a glance.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(super) struct AdminTeamDetail {
    id: String,
    display_name: String,
    status: TeamStatus,
    member_count: i64,
    last_sync_at: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(super) struct AdminTeamsResponse {
    teams: Vec<AdminTeamDetail>,
}

/// Detail row for `GET /admin/teams/{id}/members`. Trimmed versus
/// [`crate::auth::principals::UserRecord`]: exposes display identity
/// (`principal_key`, `display_name`, `display_email`) and the
/// `observed_at` timestamp from the join row, but omits `tenant_id`,
/// `object_id`, raw user-row timestamps, and any photo cache fields.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(super) struct AdminTeamMemberDetail {
    principal_key: String,
    display_name: Option<String>,
    display_email: Option<String>,
    observed_at: String,
    source: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(super) struct AdminTeamMembersResponse {
    members: Vec<AdminTeamMemberDetail>,
}

/// Admin-only: list active teams with member counts and staleness info.
#[utoipa::path(
    get,
    path = "/admin/teams",
    responses(
        (status = 200, body = AdminTeamsResponse),
        (status = 403, description = "Caller lacks SpacebotAdmin role"),
        (status = 500, description = "Instance pool unavailable or query failed"),
    ),
    tag = "admin",
)]
pub(super) async fn list_admin_teams(
    State(state): State<Arc<ApiState>>,
    auth_ctx: AuthContext,
) -> Result<Json<AdminTeamsResponse>, StatusCode> {
    require_role(&auth_ctx, ROLE_ADMIN).map_err(|_| StatusCode::FORBIDDEN)?;
    let pool = state
        .instance_pool
        .load()
        .as_ref()
        .as_ref()
        .cloned()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // LEFT JOIN so teams with zero members still appear. The aggregated
    // `MAX(observed_at)` collapses to NULL when there are no memberships,
    // which surfaces in the response as `last_sync_at: null`.
    let rows: Vec<AdminTeamRow> = match &*pool {
        crate::db::DbPool::Sqlite(p) => sqlx::query_as(
            "SELECT t.id, t.display_name, t.status, \
                    COUNT(tm.principal_key) AS member_count, \
                    MAX(tm.observed_at) AS last_sync_at \
             FROM teams t \
             LEFT JOIN team_memberships tm ON tm.team_id = t.id \
             WHERE t.status = 'active' \
             GROUP BY t.id \
             ORDER BY t.display_name, t.id",
        )
        .fetch_all(p)
        .await,
        crate::db::DbPool::Postgres(p) => sqlx::query_as(
            "SELECT t.id, t.display_name, t.status, \
                    COUNT(tm.principal_key) AS member_count, \
                    MAX(tm.observed_at) AS last_sync_at \
             FROM teams t \
             LEFT JOIN team_memberships tm ON tm.team_id = t.id \
             WHERE t.status = 'active' \
             GROUP BY t.id, t.display_name, t.status \
             ORDER BY t.display_name, t.id",
        )
        .fetch_all(p)
        .await,
    }
    .map_err(|error| {
        tracing::error!(%error, "list_admin_teams query failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let teams = rows
        .into_iter()
        .map(|row| AdminTeamDetail {
            id: row.id,
            display_name: row.display_name,
            status: row.status,
            member_count: row.member_count,
            last_sync_at: row.last_sync_at,
        })
        .collect();
    Ok(Json(AdminTeamsResponse { teams }))
}

/// Admin-only: list the user roster for a specific team. Returns 200 with
/// an empty `members: []` array when the team exists but has no
/// memberships; returns 404 for a nonexistent team id so a typo'd path
/// surfaces distinctly from a real empty team. PR #115 review finding:
/// returning 200 for both cases masked real bugs (stale link, typo in
/// team id) as "team is empty" in the admin UI.
#[utoipa::path(
    get,
    path = "/admin/teams/{id}/members",
    params(
        ("id" = String, Path, description = "Team id"),
    ),
    responses(
        (status = 200, body = AdminTeamMembersResponse),
        (status = 403, description = "Caller lacks SpacebotAdmin role"),
        (status = 404, description = "Team id does not exist"),
        (status = 500, description = "Instance pool unavailable or query failed"),
    ),
    tag = "admin",
)]
pub(super) async fn list_team_members(
    State(state): State<Arc<ApiState>>,
    auth_ctx: AuthContext,
    Path(team_id): Path<String>,
) -> Result<Json<AdminTeamMembersResponse>, StatusCode> {
    require_role(&auth_ctx, ROLE_ADMIN).map_err(|_| StatusCode::FORBIDDEN)?;
    let pool = state
        .instance_pool
        .load()
        .as_ref()
        .as_ref()
        .cloned()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Existence check: distinguish "team exists, no members" from
    // "team id does not exist." Without this the two cases collapse
    // into an identical 200 + empty array, and a typo in the URL
    // looks identical to a real empty team.
    let team_exists: Option<i64> = match &*pool {
        crate::db::DbPool::Sqlite(p) => {
            sqlx::query_scalar("SELECT 1 FROM teams WHERE id = ?")
                .bind(&team_id)
                .fetch_optional(p)
                .await
        }
        crate::db::DbPool::Postgres(p) => {
            sqlx::query_scalar("SELECT 1 FROM teams WHERE id = $1")
                .bind(&team_id)
                .fetch_optional(p)
                .await
        }
    }
    .map_err(|error| {
        tracing::error!(%error, team_id = %team_id, "list_team_members existence check failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if team_exists.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }

    let rows: Vec<AdminTeamMemberRow> = match &*pool {
        crate::db::DbPool::Sqlite(p) => sqlx::query_as(
            "SELECT u.principal_key, u.display_name, u.display_email, \
                    tm.observed_at, tm.source \
             FROM team_memberships tm \
             INNER JOIN users u ON u.principal_key = tm.principal_key \
             WHERE tm.team_id = ? \
             ORDER BY u.display_name, u.principal_key",
        )
        .bind(&team_id)
        .fetch_all(p)
        .await,
        crate::db::DbPool::Postgres(p) => sqlx::query_as(
            "SELECT u.principal_key, u.display_name, u.display_email, \
                    to_char(tm.observed_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"') AS observed_at, \
                    tm.source \
             FROM team_memberships tm \
             INNER JOIN users u ON u.principal_key = tm.principal_key \
             WHERE tm.team_id = $1 \
             ORDER BY u.display_name, u.principal_key",
        )
        .bind(&team_id)
        .fetch_all(p)
        .await,
    }
    .map_err(|error| {
        tracing::error!(%error, team_id = %team_id, "list_team_members query failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let members = rows
        .into_iter()
        .map(|row| AdminTeamMemberDetail {
            principal_key: row.principal_key,
            display_name: row.display_name,
            display_email: row.display_email,
            observed_at: row.observed_at,
            source: row.source,
        })
        .collect();
    Ok(Json(AdminTeamMembersResponse { members }))
}
