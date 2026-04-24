//! Resource-visibility handlers and shared enrichment helpers.
//!
//! Hosts two HTTP routes plus the batch-enrichment helper that list
//! handlers call post-fetch:
//!
//! - `PUT /api/resources/{resource_type}/{resource_id}/visibility` rotates a
//!   resource's visibility between Personal / Team / Org and rebinds the
//!   optional `shared_with_team_id`.
//! - `GET /api/teams` returns the active-team directory for the SPA's
//!   ShareResourceModal selector. Authenticated + SpacebotUser-gated.
//! - [`enrich_visibility_tags`] is the batch helper the memory / task /
//!   wiki / cron list handlers call to attach `VisibilityTag` to each row.
//!
//! Semantics for `set_visibility`:
//! - `check_write` gates: owner OR admin may change visibility. Non-owner
//!   non-admin gets 404 per the no-auto-broadening policy so a stranger
//!   cannot even confirm the resource exists.
//! - The handler validates the payload (visibility parse + team-without-
//!   team-id) BEFORE touching the pool, so malformed requests surface as
//!   400 Bad Request rather than 500 Internal Server Error from a CHECK
//!   constraint violation.
//! - On success, `update_visibility_only` UPDATEs the existing ownership
//!   row's `visibility` + `shared_with_team_id` fields. This preserves
//!   `owner_agent_id` + `owner_principal_key` on rotation. Non-existent
//!   rows return 404 so the endpoint cannot silently create ownership
//!   under the caller's principal.

use crate::api::state::ApiState;
use crate::auth::context::AuthContext;
use crate::auth::policy::check_write;
use crate::auth::principals::Visibility;
use crate::auth::repository::{
    get_teams_by_ids, list_ownerships_by_ids, list_teams, update_visibility_only,
};
use crate::auth::roles::{ROLE_USER, is_admin, require_role};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use std::collections::HashMap;
use std::sync::Arc;

/// Per-item enrichment attached to list responses alongside the domain type.
///
/// `visibility: None` encodes an unowned resource (no `resource_ownership`
/// row) per the no-auto-broadening policy in
/// `docs/design-docs/entra-backfill-strategy.md`. The SPA renders no chip
/// for unowned rows (`{m.visibility && <VisibilityChip />}` in the list
/// view). Defaulting to `personal` would contradict Phase 4 authz, which
/// treats unowned resources as admin-only.
///
/// Wire shape is two flat fields (`visibility`, `team_name`) because the
/// SPA's `VisibilityChip` consumes them as two independent props; nesting
/// into a discriminated enum would break the existing component API.
/// Fields are private and the invariant `team_name.is_some() ⇒ visibility
/// == Some(Visibility::Team)` is enforced at construction by the
/// [`Self::new`] builder, so callers cannot emit the illegal
/// `{visibility: Personal | Org | None, team_name: Some(_)}` shape.
#[derive(Debug, Clone, Default, Serialize, utoipa::ToSchema)]
pub struct VisibilityTag {
    /// Visibility variant or absent for unowned resources. Typed as the
    /// `Visibility` enum (not `String`) so utoipa emits a literal union
    /// in the OpenAPI schema and SPA consumers get exhaustive narrowing
    /// without a manual guard at the render site.
    #[serde(skip_serializing_if = "Option::is_none")]
    visibility: Option<Visibility>,
    /// Team display name when `visibility == Some(Visibility::Team)`; absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    team_name: Option<String>,
}

impl VisibilityTag {
    /// Construct from a `resource_ownership` row. Enforces the
    /// `team_name.is_some() ⇒ visibility == Team` invariant by dropping
    /// any `team_name` that arrives with a non-team visibility. Callers
    /// should only pass a `team_name` when `shared_with_team_id`
    /// resolved to an active team, but the narrowing is free and makes
    /// the illegal state unrepresentable on the wire regardless.
    pub fn new(visibility: Option<Visibility>, team_name: Option<String>) -> Self {
        let team_name = match visibility {
            Some(Visibility::Team) => team_name,
            _ => None,
        };
        Self {
            visibility,
            team_name,
        }
    }

    /// Lowercase serde string for the visibility variant. Consumed by
    /// the test module below to inspect the constructor's narrowing
    /// invariants.
    #[cfg(test)]
    pub(super) fn visibility(&self) -> Option<&'static str> {
        self.visibility.map(|v| v.as_str())
    }

    /// Team display name when visibility is `Team`; `None` otherwise.
    /// Consumed by the test module below to inspect the constructor's
    /// narrowing invariants.
    #[cfg(test)]
    pub(super) fn team_name(&self) -> Option<&str> {
        self.team_name.as_deref()
    }
}

/// Batch-enrich a slice of resource ids for a single resource type. Returns
/// a map from resource_id to VisibilityTag. Missing ids map to the default
/// (both fields None) so the caller can `.unwrap_or_default()` safely.
///
/// D36 pattern: cross-DB JOIN is impossible in SQLite, and 3 of 4 list
/// handlers (memories, cron, agents) use per-agent pools or in-memory
/// config, so enrichment must happen post-fetch against the instance pool.
/// Tasks' `TaskStore` does share the instance pool and could inline-JOIN,
/// but PR 1.5 chose post-fetch enrichment for all 4 handlers so readers
/// do not context-switch on which storage backs which endpoint.
pub(super) async fn enrich_visibility_tags(
    pool: &SqlitePool,
    resource_type: &str,
    resource_ids: &[String],
) -> HashMap<String, VisibilityTag> {
    // list_ownerships_by_ids short-circuits on empty input, but make it
    // explicit here so skim-reading is cheap.
    if resource_ids.is_empty() {
        return HashMap::new();
    }
    let owns = list_ownerships_by_ids(pool, resource_type, resource_ids)
        .await
        .unwrap_or_else(|error| {
            // Severity elevation: sqlx-level failures mean the instance
            // pool is broken (closed, migration mismatch, disk full).
            // The list still returns 200 with chips absent (a cosmetic
            // degradation from the user's perspective), so without
            // error-level severity SRE alerts would not fire. Blast
            // radius is every list response until the pool recovers.
            tracing::error!(
                %error,
                resource_type = %resource_type,
                count = resource_ids.len(),
                "enrich_visibility_tags: list_ownerships_by_ids failed, returning empty map (chips absent)"
            );
            HashMap::new()
        });
    let team_ids: Vec<String> = owns
        .values()
        .filter_map(|o| o.shared_with_team_id.clone())
        .collect();
    let teams = if team_ids.is_empty() {
        HashMap::new()
    } else {
        get_teams_by_ids(pool, &team_ids)
            .await
            .unwrap_or_else(|error| {
                // Same severity elevation rationale as the ownership lookup above.
                tracing::error!(
                    %error,
                    count = team_ids.len(),
                    "enrich_visibility_tags: get_teams_by_ids failed, returning empty map (team names absent)"
                );
                HashMap::new()
            })
    };
    resource_ids
        .iter()
        .map(|id| {
            let own = owns.get(id);
            // Parse the stored string into the Visibility enum. An
            // unrecognized value (e.g. a future migration leaves a row
            // with a legacy string) collapses to None rather than
            // leaking through: the SPA then renders no chip, matching
            // the no-auto-broadening policy.
            let visibility = own.and_then(|o| o.visibility_enum());
            let team_name = own
                .and_then(|o| o.shared_with_team_id.as_ref())
                .and_then(|tid| teams.get(tid).map(|t| t.display_name.clone()));
            // VisibilityTag::new drops team_name when visibility is not
            // Team, making the illegal-state pair unrepresentable.
            (id.clone(), VisibilityTag::new(visibility, team_name))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::context::{AuthContext, PrincipalType};
    use crate::auth::repository::{set_ownership, upsert_team, upsert_user_from_auth};
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect memory sqlite");
        sqlx::migrate!("./migrations/global")
            .run(&pool)
            .await
            .expect("run global migrations");
        pool
    }

    fn user_ctx(tid: &str, oid: &str) -> AuthContext {
        AuthContext {
            principal_type: PrincipalType::User,
            tid: Arc::from(tid),
            oid: Arc::from(oid),
            roles: vec![],
            groups: vec![],
            groups_overage: false,
            display_email: None,
            display_name: None,
        }
    }

    #[tokio::test]
    async fn enrich_empty_ids_returns_empty_map() {
        let pool = setup_pool().await;
        let result = enrich_visibility_tags(&pool, "memory", &[]).await;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn enrich_attaches_visibility_and_team_name_for_team_scoped_resource() {
        let pool = setup_pool().await;
        let ctx = user_ctx("t1", "alice");
        upsert_user_from_auth(&pool, &ctx).await.unwrap();
        let team = upsert_team(&pool, "grp-1", "Platform").await.unwrap();
        set_ownership(
            &pool,
            "memory",
            "m-1",
            None,
            &ctx.principal_key(),
            Visibility::Team,
            Some(&team.id),
        )
        .await
        .unwrap();

        let ids = vec!["m-1".to_string()];
        let tags = enrich_visibility_tags(&pool, "memory", &ids).await;
        let tag = tags.get("m-1").expect("m-1 present");
        assert_eq!(tag.visibility(), Some("team"));
        assert_eq!(tag.team_name(), Some("Platform"));
    }

    #[tokio::test]
    async fn enrich_missing_ownership_row_returns_none_fields_not_personal_default() {
        // Pins the D36 policy correction. A resource without an ownership
        // row must NOT default to "personal" on the wire; it must surface
        // as None so the SPA renders the fallback branch (currently
        // "Unknown" per VisibilityChip.tsx) rather than lying about
        // a visibility the backend never recorded.
        let pool = setup_pool().await;
        let ids = vec!["orphan".to_string()];
        let tags = enrich_visibility_tags(&pool, "memory", &ids).await;
        let tag = tags
            .get("orphan")
            .expect("entry present even for missing row");
        assert_eq!(
            tag.visibility(),
            None,
            "unowned resource serializes as None, not \"personal\""
        );
        assert_eq!(tag.team_name(), None);
    }

    #[tokio::test]
    async fn enrich_personal_visibility_has_no_team_name() {
        let pool = setup_pool().await;
        let ctx = user_ctx("t1", "alice");
        upsert_user_from_auth(&pool, &ctx).await.unwrap();
        set_ownership(
            &pool,
            "memory",
            "m-2",
            None,
            &ctx.principal_key(),
            Visibility::Personal,
            None,
        )
        .await
        .unwrap();

        let ids = vec!["m-2".to_string()];
        let tags = enrich_visibility_tags(&pool, "memory", &ids).await;
        let tag = tags.get("m-2").unwrap();
        assert_eq!(tag.visibility(), Some("personal"));
        assert_eq!(tag.team_name(), None);
    }

    #[test]
    fn validate_accepts_known_visibility_values() {
        let req = SetVisibilityRequest {
            visibility: "personal".into(),
            shared_with_team_id: None,
        };
        let (vis, team_id) = req.validate().unwrap();
        assert_eq!(vis.as_str(), "personal");
        assert_eq!(team_id, None);

        let req = SetVisibilityRequest {
            visibility: "team".into(),
            shared_with_team_id: Some("team-1".into()),
        };
        let (vis, team_id) = req.validate().unwrap();
        assert_eq!(vis.as_str(), "team");
        assert_eq!(team_id.as_deref(), Some("team-1"));

        let req = SetVisibilityRequest {
            visibility: "org".into(),
            shared_with_team_id: None,
        };
        let (vis, _) = req.validate().unwrap();
        assert_eq!(vis.as_str(), "org");
    }

    #[test]
    fn validate_rejects_unknown_visibility() {
        let req = SetVisibilityRequest {
            visibility: "global".into(),
            shared_with_team_id: None,
        };
        assert_eq!(req.validate(), Err(StatusCode::BAD_REQUEST));
    }

    #[test]
    fn validate_rejects_team_without_team_id() {
        let req = SetVisibilityRequest {
            visibility: "team".into(),
            shared_with_team_id: None,
        };
        assert_eq!(req.validate(), Err(StatusCode::BAD_REQUEST));
    }

    #[tokio::test]
    async fn tag_constructor_drops_team_name_when_visibility_is_not_team() {
        // Construction-time narrowing. VisibilityTag::new must reject the
        // illegal-state pair `{visibility: Personal | Org | None,
        // team_name: Some(_)}` so it cannot appear on the wire.
        assert_eq!(
            VisibilityTag::new(Some(Visibility::Personal), Some("Platform".to_string()))
                .team_name(),
            None
        );
        assert_eq!(
            VisibilityTag::new(Some(Visibility::Org), Some("Platform".to_string())).team_name(),
            None
        );
        assert_eq!(
            VisibilityTag::new(None, Some("Platform".to_string())).team_name(),
            None
        );
        // Only Team preserves team_name.
        assert_eq!(
            VisibilityTag::new(Some(Visibility::Team), Some("Platform".to_string())).team_name(),
            Some("Platform")
        );
    }
}

/// Payload accepted by `PUT /api/resources/{type}/{id}/visibility`. Keep the
/// wire shape snake_case (Rust default) so the TS client can pass
/// `{visibility, shared_with_team_id}` without custom serde rules. Visibility
/// stays stringly-typed at the deserialization boundary for forward-compat
/// (a future fourth variant added in Rust does not break existing TS clients
/// deserializing the schema). The [`Self::validate`] method does the
/// three-step translation (parse enum + enforce team-has-team-id + extract
/// the owned fields) in one place, so the handler body stays single-purpose.
#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct SetVisibilityRequest {
    visibility: String,
    #[serde(default)]
    shared_with_team_id: Option<String>,
}

impl SetVisibilityRequest {
    /// Validate the payload and project into `(Visibility, Option<team_id>)`.
    /// Centralizes the two rules the handler would otherwise inline:
    ///   1. `visibility` parses to one of the known variants.
    ///   2. A `team` visibility carries a non-None `shared_with_team_id`.
    ///
    /// Returns `Err(StatusCode::BAD_REQUEST)` on either violation so the
    /// caller can `?`-propagate without restating the error.
    fn validate(self) -> Result<(Visibility, Option<String>), StatusCode> {
        let vis = Visibility::parse(&self.visibility).ok_or(StatusCode::BAD_REQUEST)?;
        if matches!(vis, Visibility::Team) && self.shared_with_team_id.is_none() {
            return Err(StatusCode::BAD_REQUEST);
        }
        Ok((vis, self.shared_with_team_id))
    }
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
    // layer). Validation lives on SetVisibilityRequest::validate so it is
    // unit-testable independent of the whole-handler harness. The DB's
    // CHECK constraint enforces the same invariant as a belt-and-
    // suspenders defense.
    let (vis, shared_with_team_id) = req.validate()?;

    // Canonical ArcSwap peek matching the pattern in `api::me`.
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
            // is_admin is attached so operators can distinguish an
            // admin-path failure (would otherwise have bypassed
            // ownership) from a user-path failure (hints at an
            // ownership-table integrity issue).
            tracing::warn!(
                %error,
                actor = %auth_ctx.principal_key(),
                is_admin = is_admin(&auth_ctx),
                resource_type = %resource_type,
                resource_id = %resource_id,
                "authz check_write failed"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if !access.is_allowed() {
        return Err(access.to_status());
    }

    // C1 correction (PR #111 review): use `update_visibility_only` rather
    // than `set_ownership` to preserve `owner_agent_id` + `owner_principal_key`.
    // The previous UPSERT unconditionally clobbered `owner_agent_id` to None,
    // silently re-parenting agent-owned resources on every rotation. The new
    // helper UPDATEs only the visibility + team fields and returns false if
    // the row does not exist (so non-owned resources surface as 404 rather
    // than being silently claimed by the caller's principal).
    let updated = update_visibility_only(
        &pool,
        &resource_type,
        &resource_id,
        vis,
        shared_with_team_id.as_deref(),
    )
    .await
    .map_err(|error| {
        tracing::warn!(
            %error,
            actor = %auth_ctx.principal_key(),
            resource_type = %resource_type,
            resource_id = %resource_id,
            "set_visibility: update_visibility_only failed"
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if !updated {
        // `check_write` already passed, so the row must exist if we got
        // here under normal conditions. A zero-row UPDATE after a passing
        // check_write means a concurrent delete raced our rotation; treat
        // as 404 so the SPA can re-fetch and show "resource gone".
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::OK)
}

/// Minimal team projection served by `GET /api/teams`. Only `id` +
/// `display_name` cross the wire because the SPA's `ShareResourceModal`
/// renders `display_name` and sends `id` back on submit. Status is
/// filtered at the SQL layer (active-only) so it carries no useful bit,
/// and the timestamps would leak create/update cadence without adding
/// value to the Share UI. A future `/api/admin/teams` route will carry
/// a richer projection.
///
/// `#[non_exhaustive]` makes it clear within-crate callers should not
/// pattern-match on the struct literal; future additive fields
/// (member_count, last_sync_at) land without breaking them.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[non_exhaustive]
pub(super) struct TeamSummary {
    pub id: String,
    pub display_name: String,
}

/// List active teams for the Share-resource modal. Authenticated-only:
/// any signed-in user can read the team directory because every Share
/// button needs the list to populate its selector. Admin-gating would
/// break the owner-rotates-own-resource flow for non-admin users.
///
/// Inactive teams are filtered in SQL so the UI cannot offer a team
/// that would fail a follow-up `set_ownership` write (teams go inactive
/// when Graph removes them during Phase 3 sync; Phase 4 authz rejects
/// writes against inactive teams to preserve referential sanity).
#[utoipa::path(
    get,
    path = "/teams",
    responses(
        (status = 200, description = "List of active teams", body = Vec<TeamSummary>),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Authenticated but lacks SpacebotUser role"),
        (status = 500, description = "Instance pool unavailable"),
    ),
    tag = "resources",
)]
pub(super) async fn list_teams_handler(
    State(state): State<Arc<ApiState>>,
    auth_ctx: AuthContext,
) -> Result<Json<Vec<TeamSummary>>, StatusCode> {
    // Require `SpacebotUser` (or a bypass principal like LegacyStatic /
    // System). Service principals that carry only `SpacebotService`
    // are M2M identities and should not enumerate team names; a
    // dedicated scope will lift this when that consumer lands.
    require_role(&auth_ctx, ROLE_USER).map_err(|_| StatusCode::FORBIDDEN)?;

    // Canonical ArcSwap peek matching the pattern in `api::me` and
    // sibling `set_visibility` handler.
    let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() else {
        // Systemic (not per-request) failure: every subsequent request
        // hits the same branch until the pool attaches, so severity
        // matches the `list_ownerships_by_ids` / `get_teams_by_ids`
        // elevation convention.
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            "list_teams: instance_pool not attached; all /api/teams requests will 500 until pool attaches"
        );
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    };

    let rows = list_teams(&pool).await.map_err(|error| {
        tracing::error!(
            %error,
            actor = %auth_ctx.principal_key(),
            "list_teams: repository query failed"
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let out: Vec<TeamSummary> = rows
        .into_iter()
        .map(|r| TeamSummary {
            id: r.id,
            display_name: r.display_name,
        })
        .collect();
    Ok(Json(out))
}
