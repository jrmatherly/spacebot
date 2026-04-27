//! Persistence helpers for the authz data model. Thin wrappers around sqlx
//! queries. Business rules (who may read or write) land in Phase 4.
//!
//! All operations target the instance-level DB. As of Phase 11.2 the
//! `pool` parameter is `&DbPool` so each helper dispatches per-backend
//! between SQLite and Postgres. Resource-level ownership rows
//! cross-reference per-agent resources via (resource_type, resource_id),
//! which is why the instance DB cannot use FKs into per-agent tables. See
//! `docs/design-docs/entra-backfill-strategy.md` for the sweep-based orphan
//! policy this creates.
//!
//! # Backend dispatch
//!
//! Every helper that touches sqlx matches on the `DbPool` variant. SQL
//! diverges in two places: placeholder syntax (`?` vs `$N`) and the
//! `now()` expression (`strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` vs
//! `to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')`).
//! The shared `ON CONFLICT(col) DO UPDATE SET` syntax works on both
//! backends (SQLite 3.24+ adopted it; Postgres has had it since 9.5).
//!
//! Postgres SELECTs cast TIMESTAMPTZ columns through `to_char(...)` so
//! `sqlx::FromRow` derives on `UserRecord`/`TeamRecord`/
//! `ResourceOwnershipRecord` (all-`String` shapes) work identically
//! across both arms.
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
use thiserror::Error;

use crate::auth::context::AuthContext;
use crate::auth::principals::{ResourceOwnershipRecord, TeamRecord, UserRecord, Visibility};
use crate::db::DbPool;

/// Postgres timestamp-column projection used in every SELECT against the
/// instance DB so `FromRow` derives that expect `String` columns work
/// uniformly. SQLite arms keep `SELECT *` because their TEXT columns
/// already deserialize as `String`.
const PG_USERS_COLUMNS: &str = "principal_key, tenant_id, object_id, principal_type, \
    display_name, display_email, status, \
    to_char(last_seen_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"') AS last_seen_at, \
    to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"') AS created_at, \
    to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"') AS updated_at";

const PG_TEAMS_COLUMNS: &str = "id, external_id, display_name, status, \
    to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"') AS created_at, \
    to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"') AS updated_at";

const PG_RESOURCE_OWNERSHIP_COLUMNS: &str = "resource_type, resource_id, owner_agent_id, \
    owner_principal_key, visibility, shared_with_team_id, \
    to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"') AS created_at, \
    to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"') AS updated_at";

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
    pool: &DbPool,
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

    match pool {
        DbPool::Sqlite(p) => {
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
            .execute(p)
            .await?;

            let row: UserRecord = sqlx::query_as("SELECT * FROM users WHERE principal_key = ?")
                .bind(&principal_key)
                .fetch_one(p)
                .await?;
            Ok(row)
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                r#"
                INSERT INTO users (
                    principal_key, tenant_id, object_id, principal_type,
                    display_name, display_email, status, last_seen_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, 'active', to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'))
                ON CONFLICT(principal_key) DO UPDATE SET
                    display_name = excluded.display_name,
                    display_email = excluded.display_email,
                    last_seen_at = excluded.last_seen_at,
                    updated_at = to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
                "#,
            )
            .bind(&principal_key)
            .bind(ctx.tid.as_ref())
            .bind(ctx.oid.as_ref())
            .bind(principal_type)
            .bind(ctx.display_name.as_deref())
            .bind(ctx.display_email.as_deref())
            .execute(p)
            .await?;

            let row: UserRecord = sqlx::query_as(&format!(
                "SELECT {PG_USERS_COLUMNS} FROM users WHERE principal_key = $1"
            ))
            .bind(&principal_key)
            .fetch_one(p)
            .await?;
            Ok(row)
        }
    }
}

/// Upsert a team keyed by Entra group `external_id`. Called by the Graph
/// reconciliation loop when a new group is encountered.
pub async fn upsert_team(
    pool: &DbPool,
    external_id: &str,
    display_name: &str,
) -> anyhow::Result<TeamRecord> {
    let id = format!("team-{external_id}");
    match pool {
        DbPool::Sqlite(p) => {
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
            .execute(p)
            .await
            .with_context(|| format!("upsert team external_id={external_id}"))?;

            let row: TeamRecord = sqlx::query_as("SELECT * FROM teams WHERE external_id = ?")
                .bind(external_id)
                .fetch_one(p)
                .await
                .with_context(|| format!("read back team external_id={external_id}"))?;
            Ok(row)
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                r#"
                INSERT INTO teams (id, external_id, display_name, status)
                VALUES ($1, $2, $3, 'active')
                ON CONFLICT(external_id) DO UPDATE SET
                    display_name = excluded.display_name,
                    status = 'active',
                    updated_at = to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
                "#,
            )
            .bind(&id)
            .bind(external_id)
            .bind(display_name)
            .execute(p)
            .await
            .with_context(|| format!("upsert team external_id={external_id}"))?;

            let row: TeamRecord = sqlx::query_as(&format!(
                "SELECT {PG_TEAMS_COLUMNS} FROM teams WHERE external_id = $1"
            ))
            .bind(external_id)
            .fetch_one(p)
            .await
            .with_context(|| format!("read back team external_id={external_id}"))?;
            Ok(row)
        }
    }
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
    pool: &DbPool,
    principal_key: &str,
    photo_b64: Option<&str>,
) -> anyhow::Result<()> {
    let rows_affected = match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
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
            .execute(p)
            .await
            .with_context(|| format!("update user photo for {principal_key}"))?
            .rows_affected()
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                r#"
                UPDATE users
                SET display_photo_b64 = $1,
                    photo_updated_at = to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
                    updated_at = to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
                WHERE principal_key = $2
                "#,
            )
            .bind(photo_b64)
            .bind(principal_key)
            .execute(p)
            .await
            .with_context(|| format!("update user photo for {principal_key}"))?
            .rows_affected()
        }
    };
    if rows_affected == 0 {
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
    pool: &DbPool,
    resource_type: &str,
    resource_id: &str,
    owner_agent_id: Option<&str>,
    owner_principal_key: &str,
    visibility: Visibility,
    shared_with_team_id: Option<&str>,
) -> anyhow::Result<()> {
    match pool {
        DbPool::Sqlite(p) => {
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
            .execute(p)
            .await
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                r#"
                INSERT INTO resource_ownership (
                    resource_type, resource_id, owner_agent_id,
                    owner_principal_key, visibility, shared_with_team_id
                )
                VALUES ($1, $2, $3, $4, $5, $6)
                ON CONFLICT(resource_type, resource_id) DO UPDATE SET
                    owner_agent_id = excluded.owner_agent_id,
                    owner_principal_key = excluded.owner_principal_key,
                    visibility = excluded.visibility,
                    shared_with_team_id = excluded.shared_with_team_id,
                    updated_at = to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
                "#,
            )
            .bind(resource_type)
            .bind(resource_id)
            .bind(owner_agent_id)
            .bind(owner_principal_key)
            .bind(visibility.as_str())
            .bind(shared_with_team_id)
            .execute(p)
            .await
            .map(|_| Default::default())
        }
    }
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
    pool: &DbPool,
    resource_type: &str,
    resource_id: &str,
    visibility: Visibility,
    shared_with_team_id: Option<&str>,
) -> anyhow::Result<bool> {
    let rows_affected = match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
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
            .execute(p)
            .await
            .with_context(|| format!("update visibility resource={resource_type}:{resource_id}"))?
            .rows_affected()
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                r#"
                UPDATE resource_ownership
                SET visibility = $1,
                    shared_with_team_id = $2,
                    updated_at = to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
                WHERE resource_type = $3 AND resource_id = $4
                "#,
            )
            .bind(visibility.as_str())
            .bind(shared_with_team_id)
            .bind(resource_type)
            .bind(resource_id)
            .execute(p)
            .await
            .with_context(|| format!("update visibility resource={resource_type}:{resource_id}"))?
            .rows_affected()
        }
    };
    Ok(rows_affected > 0)
}

/// Read ownership. Returns `None` if the resource is not owned-tracked (e.g.,
/// pre-existing resource not yet backfilled). See
/// `docs/design-docs/entra-backfill-strategy.md` for the Phase 4 policy.
pub async fn get_ownership(
    pool: &DbPool,
    resource_type: &str,
    resource_id: &str,
) -> anyhow::Result<Option<ResourceOwnershipRecord>> {
    let row = match pool {
        DbPool::Sqlite(p) => sqlx::query_as::<_, ResourceOwnershipRecord>(
            "SELECT * FROM resource_ownership WHERE resource_type = ? AND resource_id = ?",
        )
        .bind(resource_type)
        .bind(resource_id)
        .fetch_optional(p)
        .await
        .with_context(|| format!("read ownership resource={resource_type}:{resource_id}"))?,
        DbPool::Postgres(p) => sqlx::query_as::<_, ResourceOwnershipRecord>(&format!(
            "SELECT {PG_RESOURCE_OWNERSHIP_COLUMNS} FROM resource_ownership \
             WHERE resource_type = $1 AND resource_id = $2"
        ))
        .bind(resource_type)
        .bind(resource_id)
        .fetch_optional(p)
        .await
        .with_context(|| format!("read ownership resource={resource_type}:{resource_id}"))?,
    };
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
    pool: &DbPool,
    resource_type: &str,
    resource_ids: &[String],
) -> anyhow::Result<std::collections::HashMap<String, ResourceOwnershipRecord>> {
    if resource_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    // Placeholder sequence: $1 reserves resource_type, then $2..$N for ids.
    // SQLite uses `?` everywhere.
    let dialect = pool.dialect();
    let mut bind_index: usize = 1;
    let mut next_placeholder = || -> String {
        bind_index += 1;
        match dialect {
            crate::db::Dialect::Sqlite => "?".to_string(),
            crate::db::Dialect::Postgres => format!("${bind_index}"),
        }
    };
    let id_placeholders: String = resource_ids
        .iter()
        .map(|_| next_placeholder())
        .collect::<Vec<_>>()
        .join(",");
    let rt_placeholder = match dialect {
        crate::db::Dialect::Sqlite => "?".to_string(),
        crate::db::Dialect::Postgres => "$1".to_string(),
    };

    let rows: Vec<ResourceOwnershipRecord> = match pool {
        DbPool::Sqlite(p) => {
            let query_str = format!(
                "SELECT * FROM resource_ownership \
                 WHERE resource_type = {rt_placeholder} AND resource_id IN ({id_placeholders})"
            );
            let mut q = sqlx::query_as::<_, ResourceOwnershipRecord>(&query_str).bind(resource_type);
            for id in resource_ids {
                q = q.bind(id);
            }
            q.fetch_all(p).await.with_context(|| {
                format!(
                    "batch read ownership resource_type={resource_type} count={}",
                    resource_ids.len()
                )
            })?
        }
        DbPool::Postgres(p) => {
            let query_str = format!(
                "SELECT {PG_RESOURCE_OWNERSHIP_COLUMNS} FROM resource_ownership \
                 WHERE resource_type = {rt_placeholder} AND resource_id IN ({id_placeholders})"
            );
            let mut q = sqlx::query_as::<_, ResourceOwnershipRecord>(&query_str).bind(resource_type);
            for id in resource_ids {
                q = q.bind(id);
            }
            q.fetch_all(p).await.with_context(|| {
                format!(
                    "batch read ownership resource_type={resource_type} count={}",
                    resource_ids.len()
                )
            })?
        }
    };
    Ok(rows
        .into_iter()
        .map(|r| (r.resource_id.clone(), r))
        .collect())
}

/// Batch-read team records by `id`. Used by Phase 7 list-handler enrichment
/// to resolve `shared_with_team_id` references into display names. Returns a
/// map keyed by `id`. Empty input short-circuits without a roundtrip.
pub async fn get_teams_by_ids(
    pool: &DbPool,
    team_ids: &[String],
) -> anyhow::Result<std::collections::HashMap<String, TeamRecord>> {
    if team_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let dialect = pool.dialect();
    let mut bind_index: usize = 0;
    let mut next_placeholder = || -> String {
        bind_index += 1;
        match dialect {
            crate::db::Dialect::Sqlite => "?".to_string(),
            crate::db::Dialect::Postgres => format!("${bind_index}"),
        }
    };
    let placeholders: String = team_ids
        .iter()
        .map(|_| next_placeholder())
        .collect::<Vec<_>>()
        .join(",");

    let rows: Vec<TeamRecord> = match pool {
        DbPool::Sqlite(p) => {
            let query_str = format!("SELECT * FROM teams WHERE id IN ({placeholders})");
            let mut q = sqlx::query_as::<_, TeamRecord>(&query_str);
            for id in team_ids {
                q = q.bind(id);
            }
            q.fetch_all(p)
                .await
                .with_context(|| format!("batch read teams count={}", team_ids.len()))?
        }
        DbPool::Postgres(p) => {
            let query_str = format!(
                "SELECT {PG_TEAMS_COLUMNS} FROM teams WHERE id IN ({placeholders})"
            );
            let mut q = sqlx::query_as::<_, TeamRecord>(&query_str);
            for id in team_ids {
                q = q.bind(id);
            }
            q.fetch_all(p)
                .await
                .with_context(|| format!("batch read teams count={}", team_ids.len()))?
        }
    };
    Ok(rows.into_iter().map(|r| (r.id.clone(), r)).collect())
}

/// List active teams ordered by display name with `id` as tiebreaker so
/// two teams sharing a display name produce a stable ordering. Backs
/// `GET /api/teams`, which the SPA's Share modal consumes to populate
/// its team selector. Archived teams (Graph sync removed the group)
/// are filtered at the SQL layer so the UI never offers a team that
/// would fail a downstream `set_ownership` write. The `teams.status`
/// CHECK constraint restricts values to `'active'` or `'archived'`.
///
/// Columns are listed explicitly rather than via `SELECT *` so a future
/// additive migration on `teams` does not silently widen the query's
/// result set, and the projection invariant lives at the SQL layer
/// (not just the response-mapping step).
pub async fn list_teams(pool: &DbPool) -> anyhow::Result<Vec<TeamRecord>> {
    match pool {
        DbPool::Sqlite(p) => sqlx::query_as::<_, TeamRecord>(
            "SELECT id, external_id, display_name, status, created_at, updated_at \
             FROM teams WHERE status = 'active' \
             ORDER BY display_name, id",
        )
        .fetch_all(p)
        .await
        .context("list active teams"),
        DbPool::Postgres(p) => sqlx::query_as::<_, TeamRecord>(&format!(
            "SELECT {PG_TEAMS_COLUMNS} \
             FROM teams WHERE status = 'active' \
             ORDER BY display_name, id"
        ))
        .fetch_all(p)
        .await
        .context("list active teams"),
    }
}

/// List `resource_id` values the given principal owns directly for the given
/// `resource_type`. Backs the `ResourceScope::Mine` query param on list
/// handlers: the handler fetches the owned id set here, then filters the
/// per-agent (or global) resource query by `id IN (...)`.
///
/// Returns an empty `Vec` for unknown principals and for principals with no
/// matching ownership rows. Handlers rely on this: a caller with no owned
/// resources must see an empty list, not an error.
///
/// Ordered by `resource_id` for a deterministic result across invocations.
/// Callers that need a set-membership check can `.collect::<HashSet<_>>()`.
pub async fn list_resource_ids_owned_by(
    pool: &DbPool,
    principal_key: &str,
    resource_type: &str,
) -> anyhow::Result<Vec<String>> {
    let ids: Vec<String> = match pool {
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT resource_id FROM resource_ownership \
             WHERE owner_principal_key = ? AND resource_type = ? \
             ORDER BY resource_id",
        )
        .bind(principal_key)
        .bind(resource_type)
        .fetch_all(p)
        .await
        .with_context(|| {
            format!(
                "list owned resource_ids principal_key={principal_key} resource_type={resource_type}"
            )
        })?,
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT resource_id FROM resource_ownership \
             WHERE owner_principal_key = $1 AND resource_type = $2 \
             ORDER BY resource_id",
        )
        .bind(principal_key)
        .bind(resource_type)
        .fetch_all(p)
        .await
        .with_context(|| {
            format!(
                "list owned resource_ids principal_key={principal_key} resource_type={resource_type}"
            )
        })?,
    };
    Ok(ids)
}

/// List `resource_id` values shared with any team the given principal
/// belongs to. Backs the `ResourceScope::Team` query param on list
/// handlers. Uses a single JOIN so the caller does not need a separate
/// round-trip to resolve team memberships.
///
/// The JOIN matches `resource_ownership.shared_with_team_id =
/// team_memberships.team_id`. Rows where `shared_with_team_id` is NULL
/// (i.e., `visibility IN ('personal', 'org')`) are excluded by the
/// NULL-safe INNER JOIN semantics. Excludes rows owned directly by the
/// caller (`owner_principal_key != ?`) so team-scope queries mean
/// "resources my team shared with me but that I don't own" rather than
/// doubling up with `Mine`. Handlers that want the union fetch both
/// scopes and merge.
pub async fn list_team_scoped_resource_ids(
    pool: &DbPool,
    principal_key: &str,
    resource_type: &str,
) -> anyhow::Result<Vec<String>> {
    let ids: Vec<String> = match pool {
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT DISTINCT ro.resource_id \
             FROM resource_ownership ro \
             INNER JOIN team_memberships tm ON tm.team_id = ro.shared_with_team_id \
             WHERE tm.principal_key = ? AND ro.resource_type = ? \
                   AND ro.owner_principal_key != ? \
             ORDER BY ro.resource_id",
        )
        .bind(principal_key)
        .bind(resource_type)
        .bind(principal_key)
        .fetch_all(p)
        .await
        .with_context(|| {
            format!(
                "list team-scoped resource_ids principal_key={principal_key} resource_type={resource_type}"
            )
        })?,
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT DISTINCT ro.resource_id \
             FROM resource_ownership ro \
             INNER JOIN team_memberships tm ON tm.team_id = ro.shared_with_team_id \
             WHERE tm.principal_key = $1 AND ro.resource_type = $2 \
                   AND ro.owner_principal_key != $3 \
             ORDER BY ro.resource_id",
        )
        .bind(principal_key)
        .bind(resource_type)
        .bind(principal_key)
        .fetch_all(p)
        .await
        .with_context(|| {
            format!(
                "list team-scoped resource_ids principal_key={principal_key} resource_type={resource_type}"
            )
        })?,
    };
    Ok(ids)
}
