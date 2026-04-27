//! Consolidated identity endpoint for the SPA.
//!
//! Returns everything the signed-in user needs in one payload: principal
//! identity, roles, teams, display name/email, profile photo or initials
//! fallback. Replaces the plan-era split into `/api/me` +
//! `/api/me/groups`.
//!
//! Photo handling: `display_photo_b64` is populated by the middleware's
//! fire-and-forget `sync_user_photo_for_principal` call with a 7-day
//! TTL keyed on `photo_updated_at`. When the cached row is absent or
//! null, `initials` is computed from the display name so the SPA never
//! has to branch on both absent.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;

use crate::api::state::ApiState;
use crate::auth::context::{AuthContext, PrincipalType};

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct MeResponse {
    pub principal_key: String,
    pub tid: String,
    pub oid: String,
    /// Typed enum so downstream TypeScript clients get a string-literal
    /// union (`"user" | "service_principal" | "system" | "legacy_static"`)
    /// instead of an opaque string. Snake-case serialization inherited
    /// from `PrincipalType`'s `#[serde(rename_all = "snake_case")]`.
    pub principal_type: PrincipalType,
    pub display_name: Option<String>,
    pub display_email: Option<String>,
    /// Base64 data URL for the user's Graph profile photo, or None.
    pub display_photo_data_url: Option<String>,
    /// Computed initials (1-3 chars) derived from display_name. Present
    /// when display_photo_data_url is None, so the SPA never has to
    /// branch on both absent.
    pub initials: Option<String>,
    pub roles: Vec<String>,
    pub groups: Vec<String>,
    pub groups_overage: bool,
}

#[utoipa::path(
    get,
    path = "/me",
    responses(
        (status = 200, body = MeResponse, description = "Signed-in principal identity + groups + photo"),
        (status = 401, description = "No valid authentication")
    )
)]
pub(super) async fn me(
    State(state): State<Arc<ApiState>>,
    axum::Extension(ctx): axum::Extension<AuthContext>,
) -> Json<MeResponse> {
    // Canonical ArcSwap peek. The instance pool is set atomically at
    // startup and may still be None if the daemon is running with
    // [api.auth.entra] enabled but no instance DB yet.
    let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() else {
        // Not silent: operators seeing this repeatedly in steady state
        // indicates a startup-ordering regression. Every other request
        // for the signed-in user reaching the default path is a real
        // signal, not a fallback edge case.
        tracing::warn!(
            principal = %ctx.principal_key(),
            "me: instance_pool unset; serving default identity"
        );
        return Json(default_me(&ctx));
    };

    let team_rows: Vec<(String,)> = match match &*pool {
        crate::db::DbPool::Sqlite(p) => {
            sqlx::query_as("SELECT team_id FROM team_memberships WHERE principal_key = ?")
                .bind(ctx.principal_key())
                .fetch_all(p)
                .await
        }
        crate::db::DbPool::Postgres(p) => {
            sqlx::query_as("SELECT team_id FROM team_memberships WHERE principal_key = $1")
                .bind(ctx.principal_key())
                .fetch_all(p)
                .await
        }
    } {
            Ok(rows) => rows,
            Err(err) => {
                // Pool exhaustion, schema drift, or column rename leaves
                // the user looking like they belong to no teams. Loud-log
                // the error so the "user lost all group access" report
                // has a grep target.
                tracing::warn!(
                    error = %err,
                    principal = %ctx.principal_key(),
                    "me: team_memberships query failed; returning empty groups"
                );
                Vec::new()
            }
        };

    // Read the cached photo row keyed on principal_key. The
    // photo_updated_at column is selected alongside the blob but the
    // binding is discarded here. The middleware runs its own freshness
    // check at `sync_user_photo_for_principal` before re-fetching from
    // Graph; this handler does not re-validate.
    let photo_row: Option<(Option<String>, Option<String>)> = match match &*pool {
        crate::db::DbPool::Sqlite(p) => sqlx::query_as(
            "SELECT display_photo_b64, photo_updated_at FROM users WHERE principal_key = ?",
        )
        .bind(ctx.principal_key())
        .fetch_optional(p)
        .await,
        crate::db::DbPool::Postgres(p) => sqlx::query_as(
            "SELECT display_photo_b64, \
                    to_char(photo_updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"') \
             FROM users WHERE principal_key = $1",
        )
        .bind(ctx.principal_key())
        .fetch_optional(p)
        .await,
    } {
        Ok(row) => row,
        Err(err) => {
            tracing::warn!(
                error = %err,
                principal = %ctx.principal_key(),
                "me: users photo query failed; serving without photo"
            );
            None
        }
    };

    let display_photo_data_url = photo_row.and_then(|(b64, _photo_updated_at)| {
        let b64 = b64?;
        Some(format!("data:image/jpeg;base64,{b64}"))
    });

    let display_name_owned = ctx.display_name.as_ref().map(|s| s.to_string());
    let initials = if display_photo_data_url.is_none() {
        Some(
            display_name_owned
                .as_deref()
                .map(compute_initials)
                .unwrap_or_else(|| "?".to_string()),
        )
    } else {
        None
    };

    Json(MeResponse {
        principal_key: ctx.principal_key(),
        tid: ctx.tid.to_string(),
        oid: ctx.oid.to_string(),
        principal_type: ctx.principal_type,
        display_name: display_name_owned,
        display_email: ctx.display_email.as_ref().map(|s| s.to_string()),
        display_photo_data_url,
        initials,
        roles: ctx.roles.iter().map(|r| r.to_string()).collect(),
        groups: team_rows.into_iter().map(|(id,)| id).collect(),
        groups_overage: ctx.groups_overage,
    })
}

fn default_me(ctx: &AuthContext) -> MeResponse {
    MeResponse {
        principal_key: ctx.principal_key(),
        tid: ctx.tid.to_string(),
        oid: ctx.oid.to_string(),
        principal_type: ctx.principal_type,
        display_name: None,
        display_email: None,
        display_photo_data_url: None,
        initials: Some("?".to_string()),
        roles: vec![],
        groups: vec![],
        groups_overage: false,
    }
}

/// Total function: the caller handles the None-display-name case by
/// passing a concrete `"?"` fallback rather than threading `Option`
/// through this helper. Returns `"?"` for all-whitespace input so the
/// SPA renders a stable fallback badge instead of an invisible space.
fn compute_initials(name: &str) -> String {
    let parts: Vec<&str> = name.split_whitespace().take(3).collect();
    if parts.is_empty() {
        // All-whitespace input. Return `"?"` so the SPA's avatar
        // fallback renders something legible rather than a bare space.
        return "?".to_string();
    }
    parts
        .iter()
        .filter_map(|p| p.chars().next())
        .map(|c| c.to_uppercase().to_string())
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_from_two_word_name() {
        assert_eq!(compute_initials("Alice Example"), "AE");
    }

    #[test]
    fn initials_from_three_word_name_takes_three() {
        assert_eq!(compute_initials("Alice B Example"), "ABE");
    }

    #[test]
    fn initials_from_four_word_name_truncates_to_three() {
        assert_eq!(compute_initials("Alice B Example Four"), "ABE");
    }

    #[test]
    fn initials_from_single_word_uses_first_char() {
        assert_eq!(compute_initials("alice"), "A");
    }

    #[test]
    fn initials_from_whitespace_only_returns_question_mark() {
        // Previously returned a literal space. Whitespace-only names
        // should render a stable fallback instead of an invisible badge.
        assert_eq!(compute_initials("   "), "?");
    }

    #[test]
    fn initials_from_empty_string_returns_question_mark() {
        assert_eq!(compute_initials(""), "?");
    }
}
