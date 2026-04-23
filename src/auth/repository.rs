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

/// Update the display-photo cache for an existing user (A-19). Writes
/// `display_photo_b64` (nullable, NULL when Graph returned 404) and stamps
/// `photo_updated_at = now`. Stamping on absence anchors the weekly TTL,
/// so a confirmed-absent photo is not re-fetched until the next week.
///
/// Returns `Err` when zero rows were affected: the `principal_key` row
/// does not exist yet. This can happen when the photo-sync spawn races
/// ahead of the user-upsert spawn on a user's very first authenticated
/// request. The caller (fire-and-forget from `entra_auth_middleware`)
/// logs at `warn!`; the next request will retry once the user row exists.
///
/// Returns `anyhow::Result` per the sibling pattern documented at the
/// top of the file.
pub async fn upsert_user_photo(
    pool: &SqlitePool,
    principal_key: &str,
    photo_b64: Option<&str>,
) -> anyhow::Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE users
        SET display_photo_b64 = ?,
            photo_updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        WHERE principal_key = ?
        "#,
    )
    .bind(photo_b64)
    .bind(principal_key)
    .execute(pool)
    .await
    .with_context(|| format!("update user photo for {principal_key}"))?;
    if result.rows_affected() == 0 {
        anyhow::bail!(
            "upsert_user_photo: no users row for principal_key={principal_key} \
             (photo sync raced ahead of user upsert; next request retries)"
        );
    }
    Ok(())
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

/// Rotate only the visibility + team binding on an existing ownership row
/// without touching `owner_agent_id` or `owner_principal_key`. Used by
/// Phase 7 PR 1.5's `PUT /api/resources/{type}/{id}/visibility` endpoint.
///
/// Unlike [`set_ownership`] which UPSERTs all fields (correct at creation
/// time), this helper is for "the owner or an admin is reclassifying an
/// existing resource." Preserving `owner_agent_id` is critical because
/// an admin rotating an agent-owned memory from Team to Org must not
/// strip the agent link that allows the agent to keep reading the row.
///
/// Returns `Ok(false)` when no row exists for `(resource_type, resource_id)`
/// so the caller can return 404 rather than accidentally creating an
/// ownership row under the caller's principal (which would be a silent
/// re-parent of a pre-existing unowned resource to the wrong owner).
pub async fn update_visibility_only(
    pool: &SqlitePool,
    resource_type: &str,
    resource_id: &str,
    visibility: Visibility,
    shared_with_team_id: Option<&str>,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE resource_ownership
        SET visibility = ?,
            shared_with_team_id = ?,
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        WHERE resource_type = ? AND resource_id = ?
        "#,
    )
    .bind(visibility.as_str())
    .bind(shared_with_team_id)
    .bind(resource_type)
    .bind(resource_id)
    .execute(pool)
    .await
    .with_context(|| format!("update visibility resource={resource_type}:{resource_id}"))?;
    Ok(result.rows_affected() > 0)
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

/// Batch-read ownership rows for a list of resources of the same type. Used by
/// Phase 7 list-handler enrichment where a single page of memories/cron/agents
/// needs its visibility + team-binding looked up in one roundtrip. Returns a
/// map keyed by `resource_id` so callers can `.get(&row.id)` during response
/// assembly. Missing entries mean "no ownership row", and the handler decides
/// how to render that. The SPA treats `None` as "Legacy" and never defaults
/// to "personal" per the no-auto-broadening policy in
/// `entra-backfill-strategy.md`.
///
/// Empty `resource_ids` returns an empty map without hitting the pool. Binds
/// one placeholder per id, following the pattern proven in
/// `MemoryStore::get_associations_between` for SQLite variadic IN clauses.
/// Cited by function name rather than line number so the citation survives
/// unrelated edits to the memory store.
pub async fn list_ownerships_by_ids(
    pool: &SqlitePool,
    resource_type: &str,
    resource_ids: &[String],
) -> anyhow::Result<std::collections::HashMap<String, ResourceOwnershipRecord>> {
    if resource_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders: String = resource_ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");
    let query_str = format!(
        "SELECT * FROM resource_ownership \
         WHERE resource_type = ? AND resource_id IN ({placeholders})"
    );
    let mut query = sqlx::query_as::<_, ResourceOwnershipRecord>(&query_str).bind(resource_type);
    for id in resource_ids {
        query = query.bind(id);
    }
    let rows = query.fetch_all(pool).await.with_context(|| {
        format!(
            "batch read ownership resource_type={resource_type} count={}",
            resource_ids.len()
        )
    })?;
    Ok(rows
        .into_iter()
        .map(|r| (r.resource_id.clone(), r))
        .collect())
}

/// Batch-read team records by `id`. Used by Phase 7 list-handler enrichment
/// to resolve `shared_with_team_id` references into display names. Returns a
/// map keyed by `id`. Empty input short-circuits without a roundtrip.
pub async fn get_teams_by_ids(
    pool: &SqlitePool,
    team_ids: &[String],
) -> anyhow::Result<std::collections::HashMap<String, TeamRecord>> {
    if team_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders: String = team_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let query_str = format!("SELECT * FROM teams WHERE id IN ({placeholders})");
    let mut query = sqlx::query_as::<_, TeamRecord>(&query_str);
    for id in team_ids {
        query = query.bind(id);
    }
    let rows = query
        .fetch_all(pool)
        .await
        .with_context(|| format!("batch read teams count={}", team_ids.len()))?;
    Ok(rows.into_iter().map(|r| (r.id.clone(), r)).collect())
}

/// List active teams ordered by display name. Backs `GET /api/teams`, which
/// the SPA's Share modal consumes to populate its team selector. Archived
/// teams (Graph sync removed the group) are filtered at the SQL layer so
/// the UI never offers a team that would fail a downstream `set_ownership`
/// write. The `teams.status` CHECK constraint restricts values to
/// `'active'` or `'archived'`.
pub async fn list_teams(pool: &SqlitePool) -> anyhow::Result<Vec<TeamRecord>> {
    sqlx::query_as::<_, TeamRecord>(
        "SELECT * FROM teams WHERE status = 'active' ORDER BY display_name",
    )
    .fetch_all(pool)
    .await
    .context("list active teams")
}
