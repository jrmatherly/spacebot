//! Persistence helpers for the authz data model. These are thin wrappers
//! around sqlx queries; business rules (who may read/write) land in Phase 4.
//!
//! All operations target the instance-level DB (`SqlitePool` for `spacebot.db`),
//! NOT a per-agent DB. Resource-level ownership rows cross-reference
//! per-agent resources via (resource_type, resource_id).

use sqlx::SqlitePool;

use crate::auth::context::AuthContext;
use crate::auth::principals::{ResourceOwnershipRecord, TeamRecord, UserRecord, Visibility};

/// Upsert the user record on each successful login. Refreshes display
/// fields from the latest token, bumps `last_seen_at`. Never changes
/// identity keys.
pub async fn upsert_user_from_auth(
    pool: &SqlitePool,
    ctx: &AuthContext,
) -> sqlx::Result<UserRecord> {
    use crate::auth::context::PrincipalType;
    let principal_type = match ctx.principal_type {
        PrincipalType::User => "user",
        PrincipalType::ServicePrincipal => "service_principal",
        PrincipalType::System => "system",
        PrincipalType::LegacyStatic => {
            return Err(sqlx::Error::Protocol(
                "legacy_static principals do not have user rows".into(),
            ));
        }
    };
    let principal_key = ctx.principal_key();

    sqlx::query(
        r#"
        INSERT INTO users (
            principal_key, tenant_id, object_id, principal_type,
            display_name, display_email, status, last_seen_at
        )
        VALUES (?, ?, ?, ?, ?, ?, 'active', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        ON CONFLICT(principal_key) DO UPDATE SET
            display_name = excluded.display_name,
            display_email = excluded.display_email,
            last_seen_at = excluded.last_seen_at,
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        "#,
    )
    .bind(&principal_key)
    .bind(ctx.tid.as_ref())
    .bind(ctx.oid.as_ref())
    .bind(principal_type)
    .bind(ctx.display_name.as_deref().map(|s| s.to_string()))
    .bind(ctx.display_email.as_deref().map(|s| s.to_string()))
    .execute(pool)
    .await?;

    let row: UserRecord = sqlx::query_as("SELECT * FROM users WHERE principal_key = ?")
        .bind(&principal_key)
        .fetch_one(pool)
        .await?;
    Ok(row)
}

/// Upsert a team keyed by Entra group `external_id`. Called by Phase 3's
/// reconciliation when a new group is encountered.
pub async fn upsert_team(
    pool: &SqlitePool,
    external_id: &str,
    display_name: &str,
) -> sqlx::Result<TeamRecord> {
    let id = format!("team-{external_id}");
    sqlx::query(
        r#"
        INSERT INTO teams (id, external_id, display_name, status)
        VALUES (?, ?, ?, 'active')
        ON CONFLICT(external_id) DO UPDATE SET
            display_name = excluded.display_name,
            status = 'active',
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        "#,
    )
    .bind(&id)
    .bind(external_id)
    .bind(display_name)
    .execute(pool)
    .await?;

    let row: TeamRecord = sqlx::query_as("SELECT * FROM teams WHERE external_id = ?")
        .bind(external_id)
        .fetch_one(pool)
        .await?;
    Ok(row)
}

/// Upsert resource ownership at resource-creation time.
/// Callers: every handler that creates an owned resource (Phase 4).
pub async fn set_ownership(
    pool: &SqlitePool,
    resource_type: &str,
    resource_id: &str,
    owner_agent_id: Option<&str>,
    owner_principal_key: &str,
    visibility: Visibility,
    shared_with_team_id: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO resource_ownership (
            resource_type, resource_id, owner_agent_id,
            owner_principal_key, visibility, shared_with_team_id
        )
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(resource_type, resource_id) DO UPDATE SET
            owner_agent_id = excluded.owner_agent_id,
            owner_principal_key = excluded.owner_principal_key,
            visibility = excluded.visibility,
            shared_with_team_id = excluded.shared_with_team_id,
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        "#,
    )
    .bind(resource_type)
    .bind(resource_id)
    .bind(owner_agent_id)
    .bind(owner_principal_key)
    .bind(visibility.as_str())
    .bind(shared_with_team_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Read ownership. Returns None if the resource is not owned-tracked (e.g.,
/// pre-existing resource not yet backfilled — see backfill doc in Task 2.8).
pub async fn get_ownership(
    pool: &SqlitePool,
    resource_type: &str,
    resource_id: &str,
) -> sqlx::Result<Option<ResourceOwnershipRecord>> {
    let row = sqlx::query_as::<_, ResourceOwnershipRecord>(
        "SELECT * FROM resource_ownership WHERE resource_type = ? AND resource_id = ?",
    )
    .bind(resource_type)
    .bind(resource_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}
