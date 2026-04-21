//! Persistence helpers for the authz data model. Thin wrappers around sqlx
//! queries. Business rules (who may read or write) land in Phase 4.
//!
//! All operations target the instance-level DB (`SqlitePool` for `spacebot.db`),
//! NOT a per-agent DB. Resource-level ownership rows cross-reference
//! per-agent resources via (resource_type, resource_id), which is why the
//! instance DB cannot use FKs into per-agent tables. See
//! `docs/design-docs/entra-backfill-strategy.md` for the sweep-based orphan
//! policy this creates.
//!
//! # Error taxonomy
//!
//! [`upsert_user_from_auth`] returns [`RepositoryError`] because it has a
//! genuine domain precondition (legacy-static principals are not persisted)
//! that callers need to distinguish from sqlx failures. Match on
//! `RepositoryError::InvalidPrincipalType` for that path.
//!
//! [`upsert_team`], [`set_ownership`], and [`get_ownership`] have no such
//! precondition and return [`anyhow::Result`] with `.with_context()` frames.
//! Callers that need sqlx-variant classification (CHECK violation vs FK
//! violation vs transient) use `anyhow::Error::downcast_ref::<sqlx::Error>()`
//! instead of a match. The downcast walks the context chain and recovers the
//! original `sqlx::Error`.

use anyhow::Context as _;
use sqlx::SqlitePool;
use thiserror::Error;

use crate::auth::context::AuthContext;
use crate::auth::principals::{ResourceOwnershipRecord, TeamRecord, UserRecord, Visibility};

/// Errors returned by this module. Wraps `sqlx::Error` but carries a distinct
/// variant for the one domain-level invariant we enforce here: legacy-static
/// principals cannot be persisted as user rows. Callers that need finer
/// classification of sqlx errors (CHECK vs FK vs transient) should match on
/// `Sqlx(e)` and inspect `e.as_database_error()`.
#[derive(Debug, Error)]
pub enum RepositoryError {
    /// The caller passed a `PrincipalType::LegacyStatic` to a helper that only
    /// persists real Entra principals. This is a programmer error, not a
    /// runtime failure. Handlers should not retry.
    #[error("legacy_static principals do not have user rows")]
    InvalidPrincipalType,

    /// Underlying sqlx error. Contains the original variant for classification.
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

/// Upsert the user record on each successful login. Refreshes display fields
/// from the latest token and bumps `last_seen_at`. Never changes identity keys.
///
/// Returns [`RepositoryError::InvalidPrincipalType`] for
/// [`PrincipalType::LegacyStatic`][crate::auth::context::PrincipalType::LegacyStatic].
/// Those principals do not get user rows. Callers should not retry this error.
pub async fn upsert_user_from_auth(
    pool: &SqlitePool,
    ctx: &AuthContext,
) -> Result<UserRecord, RepositoryError> {
    use crate::auth::context::PrincipalType;
    let principal_type = match ctx.principal_type {
        PrincipalType::User => "user",
        PrincipalType::ServicePrincipal => "service_principal",
        PrincipalType::System => "system",
        PrincipalType::LegacyStatic => return Err(RepositoryError::InvalidPrincipalType),
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
    .bind(ctx.display_name.as_deref())
    .bind(ctx.display_email.as_deref())
    .execute(pool)
    .await?;

    let row: UserRecord = sqlx::query_as("SELECT * FROM users WHERE principal_key = ?")
        .bind(&principal_key)
        .fetch_one(pool)
        .await?;
    Ok(row)
}

/// Upsert a team keyed by Entra group `external_id`. Called by the Graph
/// reconciliation loop when a new group is encountered.
pub async fn upsert_team(
    pool: &SqlitePool,
    external_id: &str,
    display_name: &str,
) -> anyhow::Result<TeamRecord> {
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
    .await
    .with_context(|| format!("upsert team external_id={external_id}"))?;

    let row: TeamRecord = sqlx::query_as("SELECT * FROM teams WHERE external_id = ?")
        .bind(external_id)
        .fetch_one(pool)
        .await
        .with_context(|| format!("read back team external_id={external_id}"))?;
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
) -> anyhow::Result<()> {
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
    .await
    .with_context(|| {
        format!("set ownership resource={resource_type}:{resource_id} owner={owner_principal_key}")
    })?;
    Ok(())
}

/// Read ownership. Returns `None` if the resource is not owned-tracked (e.g.,
/// pre-existing resource not yet backfilled). See
/// `docs/design-docs/entra-backfill-strategy.md` for the Phase 4 policy.
pub async fn get_ownership(
    pool: &SqlitePool,
    resource_type: &str,
    resource_id: &str,
) -> anyhow::Result<Option<ResourceOwnershipRecord>> {
    let row = sqlx::query_as::<_, ResourceOwnershipRecord>(
        "SELECT * FROM resource_ownership WHERE resource_type = ? AND resource_id = ?",
    )
    .bind(resource_type)
    .bind(resource_id)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("read ownership resource={resource_type}:{resource_id}"))?;
    Ok(row)
}
