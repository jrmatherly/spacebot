//! Consolidated identity endpoint for the SPA.
//!
//! Returns everything the signed-in user needs in one payload: principal
//! identity, roles, teams, display name/email, profile photo or initials
//! fallback. Replaces the Phase-6-era plan to ship `/api/me/groups`
//! separately (A-18).
//!
//! Photo handling (A-19): `display_photo_b64` is populated by the
//! middleware's fire-and-forget `sync_user_photo_for_principal` call
//! (weekly TTL via `photo_updated_at`). When the cached row is absent
//! or null, `initials` is computed from the display name so the SPA
//! never has to branch on both absent.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;

use crate::api::state::ApiState;
use crate::auth::context::AuthContext;

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct MeResponse {
    pub principal_key: String,
    pub tid: String,
    pub oid: String,
    pub principal_type: String,
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
    // Canonical ArcSwap peek — the instance pool is set atomically at
    // startup and may still be None if the daemon is running with
    // [api.auth.entra] enabled but no instance DB yet.
    let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() else {
        return Json(default_me(&ctx));
    };

    let team_rows: Vec<(String,)> =
        sqlx::query_as("SELECT team_id FROM team_memberships WHERE principal_key = ?")
            .bind(ctx.principal_key())
            .fetch_all(&pool)
            .await
            .unwrap_or_default();

    // A-19: read the cached photo row keyed on principal_key.
    // Column names match migration 20260420120006_users_photo.sql:
    // `display_photo_b64` (TEXT, nullable) + `photo_updated_at` (TEXT,
    // nullable). The `_photo_updated_at` result is unused here but the
    // column is present for the middleware's weekly-TTL freshness check
    // to inspect before re-fetching from Graph.
    let photo_row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT display_photo_b64, photo_updated_at FROM users WHERE principal_key = ?",
    )
    .bind(ctx.principal_key())
    .fetch_optional(&pool)
    .await
    .unwrap_or(None);

    let display_photo_data_url = photo_row.and_then(|(b64, _photo_updated_at)| {
        let b64 = b64?;
        Some(format!("data:image/jpeg;base64,{b64}"))
    });

    let display_name_owned = ctx.display_name.as_deref().map(|s| s.to_string());
    let initials = if display_photo_data_url.is_none() {
        Some(compute_initials(display_name_owned.as_deref()))
    } else {
        None
    };

    Json(MeResponse {
        principal_key: ctx.principal_key(),
        tid: ctx.tid.to_string(),
        oid: ctx.oid.to_string(),
        // Canonical snake_case via as_canonical_str (Phase 5 Task 5.6).
        principal_type: ctx.principal_type.as_canonical_str().to_string(),
        display_name: display_name_owned,
        display_email: ctx.display_email.as_deref().map(|s| s.to_string()),
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
        principal_type: ctx.principal_type.as_canonical_str().to_string(),
        display_name: None,
        display_email: None,
        display_photo_data_url: None,
        initials: Some("?".to_string()),
        roles: vec![],
        groups: vec![],
        groups_overage: false,
    }
}

fn compute_initials(name: Option<&str>) -> String {
    let Some(name) = name else {
        return "?".to_string();
    };
    let parts: Vec<&str> = name.split_whitespace().take(3).collect();
    if parts.is_empty() {
        return name.chars().take(1).collect::<String>().to_uppercase();
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
        assert_eq!(compute_initials(Some("Alice Example")), "AE");
    }

    #[test]
    fn initials_from_three_word_name_takes_three() {
        assert_eq!(compute_initials(Some("Alice B Example")), "ABE");
    }

    #[test]
    fn initials_from_four_word_name_truncates_to_three() {
        assert_eq!(compute_initials(Some("Alice B Example Four")), "ABE");
    }

    #[test]
    fn initials_from_single_word_uses_first_char() {
        assert_eq!(compute_initials(Some("alice")), "A");
    }

    #[test]
    fn initials_from_none_is_question_mark() {
        assert_eq!(compute_initials(None), "?");
    }

    #[test]
    fn initials_from_whitespace_only_uses_first_char() {
        // split_whitespace yields empty parts vec, falls through to
        // the chars().take(1) branch.
        assert_eq!(compute_initials(Some("  ")), " ");
    }
}
