//! Per-resource access policy. Called from handlers; consults
//! `resource_ownership` and `team_memberships`.
//!
//! The [`Access`] enum discriminates:
//!   - [`Access::Allowed`] → handler proceeds.
//!   - [`Access::Denied`]`([DenyReason::NotOwned])` → handler returns 404
//!     (the resource has no ownership row: either pre-Entra data or truly
//!     missing; either way, do not leak existence).
//!   - [`Access::Denied`]`([DenyReason::NotYours])` → handler returns 404
//!     (hide existence from principals who don't own the resource).
//!   - [`Access::Denied`]`([DenyReason::Forbidden])` → handler returns 403
//!     (role-based deny; resource's existence was already proved by a prior
//!     step, so there's nothing to hide).
//!
//! The `check_read` / `check_write` functions are async because they read
//! the `resource_ownership` + `team_memberships` tables. Callers should
//! pass the instance-level `SqlitePool` (the one `ApiState.instance_pool`
//! holds). These tables do NOT live in the per-agent databases.

use sqlx::SqlitePool;

use crate::auth::context::{AuthContext, PrincipalType};
use crate::auth::principals::{ResourceOwnershipRecord, Visibility};
use crate::auth::repository::get_ownership;
use crate::auth::roles::is_admin;

/// Access decision returned by [`check_read`] / [`check_write`].
///
/// `#[must_use]` enforces that callers actually inspect the result.
/// Forgetting to consult an `Access` value after computing one is the
/// exact class of bug ("called check_read, discarded result") that
/// converts this type into a no-op at compile time.
#[must_use = "Access value must be checked; discarding it bypasses authz"]
#[derive(Debug, Clone)]
pub enum Access {
    Allowed,
    Denied(DenyReason),
}

/// Why a request was denied. Maps to HTTP status codes via
/// [`Access::to_status`]; `NotOwned`/`NotYours` both become 404 so a
/// non-owner cannot distinguish "resource doesn't exist" from "resource
/// exists but you can't see it".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DenyReason {
    /// No `resource_ownership` row exists for this resource. Could be
    /// pre-Entra data or a truly missing resource.
    NotOwned,
    /// Ownership exists but the requester is not the owner and has no
    /// visibility path to it (Personal -> not owner; Team -> not a member).
    NotYours,
    /// Role-based deny (requester is authenticated but lacks the required
    /// role). Resource existence already established.
    ///
    /// Not currently constructed by `check_read`/`check_write` (which
    /// only produce `NotOwned`/`NotYours`). Kept as part of the public
    /// enum because Phase 5's audit log keys off `Denied(Forbidden)` for
    /// the `AuthzDenied` event type, and future role-gated helpers
    /// (e.g. `require_role_for_resource`) will produce this variant.
    /// See `.scratchpad/plans/entraid-auth/phase-5-audit-log.md` for the
    /// Phase 5 consumer.
    Forbidden,
}

impl Access {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Access::Allowed)
    }

    /// Map a decision to an HTTP status code. `NotOwned` and `NotYours`
    /// both map to 404 to avoid leaking resource existence. `Forbidden`
    /// (role deny) maps to 403 because the resource already proved its
    /// existence via a prior step.
    pub fn to_status(&self) -> axum::http::StatusCode {
        match self {
            Access::Allowed => axum::http::StatusCode::OK,
            Access::Denied(DenyReason::Forbidden) => axum::http::StatusCode::FORBIDDEN,
            Access::Denied(DenyReason::NotOwned | DenyReason::NotYours) => {
                axum::http::StatusCode::NOT_FOUND
            }
        }
    }
}

/// Decide read access to a resource. Admins and system/legacy-static
/// principals bypass per the matrix in
/// `docs/design-docs/entra-role-permission-matrix.md`. Everyone else must
/// either own the resource directly or reach it via visibility (team
/// membership for `Visibility::Team`, anyone for `Visibility::Org`).
pub async fn check_read(
    pool: &SqlitePool,
    ctx: &AuthContext,
    resource_type: &str,
    resource_id: &str,
) -> anyhow::Result<Access> {
    if is_admin(ctx) {
        return Ok(Access::Allowed);
    }

    let own = get_ownership(pool, resource_type, resource_id).await?;
    let Some(own) = own else {
        return Ok(Access::Denied(DenyReason::NotOwned));
    };

    match ctx.principal_type {
        PrincipalType::System | PrincipalType::LegacyStatic => Ok(Access::Allowed),
        PrincipalType::User | PrincipalType::ServicePrincipal => {
            decide_user_read(pool, ctx, &own).await
        }
    }
}

async fn decide_user_read(
    pool: &SqlitePool,
    ctx: &AuthContext,
    own: &ResourceOwnershipRecord,
) -> anyhow::Result<Access> {
    if own.owner_principal_key == ctx.principal_key() {
        return Ok(Access::Allowed);
    }
    let Some(vis) = own.visibility_enum() else {
        // Invalid visibility string in DB. The CHECK constraint at
        // `migrations/global/20260420120003_resource_ownership.sql` should
        // prevent this, so reaching this branch means migration skew, DB
        // tampering, or a sqlite feature-flag difference. Surfacing as 500
        // instead of Denied(NotYours) converts an integrity bug into a
        // visible operator alert rather than a silent "resource not found"
        // UX bug that would take weeks of debugging to attribute.
        return Err(anyhow::anyhow!(
            "resource_ownership integrity violation: invalid visibility \
             value {:?} for {}:{}",
            own.visibility,
            own.resource_type,
            own.resource_id,
        ));
    };
    match vis {
        Visibility::Personal => Ok(Access::Denied(DenyReason::NotYours)),
        Visibility::Org => Ok(Access::Allowed),
        Visibility::Team => {
            let Some(team_id) = own.shared_with_team_id.as_ref() else {
                // Same reasoning: the CHECK constraint enforces team_id
                // presence when visibility = 'team'. Missing team_id here
                // is an integrity bug, not a deny.
                return Err(anyhow::anyhow!(
                    "resource_ownership integrity violation: Team visibility \
                     with null shared_with_team_id for {}:{}",
                    own.resource_type,
                    own.resource_id,
                ));
            };
            let found: Option<i64> = sqlx::query_scalar(
                r#"
                SELECT 1 FROM team_memberships
                WHERE principal_key = ? AND team_id = ?
                "#,
            )
            .bind(ctx.principal_key())
            .bind(team_id)
            .fetch_optional(pool)
            .await?;
            if found.is_some() {
                Ok(Access::Allowed)
            } else {
                Ok(Access::Denied(DenyReason::NotYours))
            }
        }
    }
}

/// Variant of [`check_read`] that additionally reports whether the allow
/// was an admin break-glass (admin reading another user's resource).
/// Handlers wire this into the audit log: when `admin_override` is true
/// and the decision is `Allowed`, emit an `admin_<verb>` event per the
/// matrix at `docs/design-docs/entra-role-permission-matrix.md`.
///
/// `admin_override` is set ONLY when a principal with the `SpacebotAdmin`
/// role reads another user's resource. `LegacyStatic` (pre-Entra CI and
/// scripts) and `System` (Cortex-initiated) bypass `check_read` via
/// [`is_admin`] but MUST NOT trip the audit flag: the matrix lists those
/// principals as plain "yes" while `Admin` is "yes (audited)". Mis-setting
/// `admin_override` here would pollute the quarterly access review with
/// continuous false-positive break-glass entries once Phase 5 wires
/// `AuditAppender::append`.
///
/// Phase 4 stubs the audit side at `tracing::info!`. Phase 5 replaces
/// that with an `AuditAppender` call against the hash-chained audit log.
pub async fn check_read_with_audit(
    pool: &SqlitePool,
    ctx: &AuthContext,
    resource_type: &str,
    resource_id: &str,
) -> anyhow::Result<(Access, bool)> {
    let own = get_ownership(pool, resource_type, resource_id).await?;
    let Some(own) = own else {
        return Ok((Access::Denied(DenyReason::NotOwned), false));
    };
    let is_owner = own.owner_principal_key == ctx.principal_key();
    // Role-Admin only: Legacy/System bypass without audit. See doc above.
    let admin_override = ctx.has_role(crate::auth::roles::ROLE_ADMIN) && !is_owner;

    let decision = if is_admin(ctx) {
        Access::Allowed
    } else {
        match ctx.principal_type {
            PrincipalType::System | PrincipalType::LegacyStatic => Access::Allowed,
            PrincipalType::User | PrincipalType::ServicePrincipal => {
                decide_user_read(pool, ctx, &own).await?
            }
        }
    };
    Ok((decision, admin_override))
}

/// Cross-agent link policy: the acting principal must have read access to
/// both the calling agent and the peer agent. Admin/system/legacy-static
/// bypass applies because they bypass [`check_read`]. Returns `true` when
/// the link is allowed. Call sites on deny should drop the routed message
/// and log at `warn!` with the actor principal key.
///
/// `#[must_use]` exists because the helper is useless if the boolean is
/// discarded: the enforcement surface at `src/tools/send_agent_message.rs`
/// (deferred to PR 2) has a TODO marker exactly here, so the compile-time
/// guard makes "forgot to enforce" a compile error in that future wiring.
#[must_use = "can_link_channel returns a boolean that MUST gate cross-agent dispatch"]
pub async fn can_link_channel(
    pool: &SqlitePool,
    ctx: &AuthContext,
    self_agent_id: &str,
    peer_agent_id: &str,
) -> anyhow::Result<bool> {
    let a = check_read(pool, ctx, "agent", self_agent_id).await?;
    let b = check_read(pool, ctx, "agent", peer_agent_id).await?;
    Ok(a.is_allowed() && b.is_allowed())
}

/// Decide write access to a resource. Stricter than read: team-visibility
/// resources are read-shared but writable only by the owner (and admins).
pub async fn check_write(
    pool: &SqlitePool,
    ctx: &AuthContext,
    resource_type: &str,
    resource_id: &str,
) -> anyhow::Result<Access> {
    if is_admin(ctx) {
        return Ok(Access::Allowed);
    }
    let own = get_ownership(pool, resource_type, resource_id).await?;
    let Some(own) = own else {
        return Ok(Access::Denied(DenyReason::NotOwned));
    };
    if own.owner_principal_key == ctx.principal_key() {
        Ok(Access::Allowed)
    } else {
        Ok(Access::Denied(DenyReason::NotYours))
    }
}
