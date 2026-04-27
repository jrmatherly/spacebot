//! Ownership-scoped lookups: which agent_ids can a given AuthContext see?
//!
//! Returns the visible agent_id list for a principal across the three
//! ownership paths the resource_ownership table encodes: `personal`
//! (owner_principal_key match), `team` (resolved via team_memberships
//! JOIN, encapsulated in `auth::repository::list_team_scoped_resource_ids`),
//! and `org` (anyone). Admins (ROLE_ADMIN, legacy_static) see every
//! agent_id; the caller is responsible for emitting an audit row when an
//! admin reads a non-owned resource.
//!
//! Returns `Vec<String>` of agent_ids rather than typed pool refs because
//! `ApiState.agent_pools` is currently `HashMap<String, sqlx::SqlitePool>`.
//! Phase 11.3 widens that to `HashMap<String, Arc<DbPool>>`; the typed
//! sibling `pools_for(ctx)` lands as WS-4 Task 4.5 once the typing is
//! in place. This helper composes `list_team_scoped_resource_ids` rather
//! than duplicating the team-membership JOIN.

use crate::auth::AuthContext;
use crate::auth::repository::list_team_scoped_resource_ids;
use crate::auth::roles::is_admin;
use crate::db::DbPool;

use anyhow::Context as _;

const AGENT_RESOURCE_TYPE: &str = "agent";

/// Returns the agent_ids visible to `ctx`. Admins see every agent;
/// non-admin users see the union of (owned, team-shared, org-visible).
///
/// Caller responsibility: when `is_admin(ctx)` is true and the result is
/// used to expose a non-owned resource, emit an audit row via
/// `crate::auth::policy::fire_admin_read_audit`.
pub async fn agent_ids_for(ctx: &AuthContext, pool: &DbPool) -> anyhow::Result<Vec<String>> {
    if is_admin(ctx) {
        return list_all_agent_ids(pool).await;
    }

    let principal_key = ctx.principal_key();
    let mut visible = list_owned_or_org_agent_ids(pool, &principal_key).await?;
    let team_shared =
        list_team_scoped_resource_ids(pool, &principal_key, AGENT_RESOURCE_TYPE).await?;
    visible.extend(team_shared);
    visible.sort();
    visible.dedup();
    Ok(visible)
}

async fn list_all_agent_ids(pool: &DbPool) -> anyhow::Result<Vec<String>> {
    match pool {
        DbPool::Sqlite(p) => sqlx::query_scalar::<_, String>(
            "SELECT resource_id FROM resource_ownership WHERE resource_type = 'agent'",
        )
        .fetch_all(p)
        .await
        .context("list all agent ids (sqlite)"),
        DbPool::Postgres(p) => sqlx::query_scalar::<_, String>(
            "SELECT resource_id FROM resource_ownership WHERE resource_type = 'agent'",
        )
        .fetch_all(p)
        .await
        .context("list all agent ids (postgres)"),
    }
}

async fn list_owned_or_org_agent_ids(
    pool: &DbPool,
    principal_key: &str,
) -> anyhow::Result<Vec<String>> {
    // Owned-by-principal OR org-visible. Team-shared rows are fetched
    // separately by the caller via list_team_scoped_resource_ids.
    match pool {
        DbPool::Sqlite(p) => sqlx::query_scalar::<_, String>(
            "SELECT resource_id FROM resource_ownership \
             WHERE resource_type = 'agent' \
               AND (owner_principal_key = ? OR visibility = 'org')",
        )
        .bind(principal_key)
        .fetch_all(p)
        .await
        .context("list owned-or-org agent ids (sqlite)"),
        DbPool::Postgres(p) => sqlx::query_scalar::<_, String>(
            "SELECT resource_id FROM resource_ownership \
             WHERE resource_type = 'agent' \
               AND (owner_principal_key = $1 OR visibility = 'org')",
        )
        .bind(principal_key)
        .fetch_all(p)
        .await
        .context("list owned-or-org agent ids (postgres)"),
    }
}
