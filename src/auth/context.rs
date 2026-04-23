//! Per-request auth context. Populated by the JWT or static-token middleware
//! and consumed by per-handler authz helpers in Phase 4+.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use std::sync::Arc;

/// Stable identifier category for the authenticated principal. The four
/// variants map to disjoint authz paths: human users authenticate via
/// delegated Entra tokens, service principals via client-credentials
/// grant, the system (cortex) principal constructs internally, and the
/// static-token branch represents operator-level coarse access.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalType {
    /// A human user, identified by Entra `tid` + `oid`.
    User,
    /// A Spacebot-internal system principal (e.g., the Cortex).
    /// Not issued an Entra JWT; constructed internally.
    System,
    /// An application / service principal using the client-credentials grant.
    /// Identified by Entra `tid` + `oid` (app identity) with no `scp` claim.
    ServicePrincipal,
    /// The static `auth_token` legacy branch. Carries no Entra identity.
    /// Authz for this principal is coarse: full access, no per-user scoping.
    LegacyStatic,
}

impl PrincipalType {
    /// Canonical snake_case string form used across the codebase wherever
    /// the principal type is serialized as free-text: audit events
    /// (`audit_events.principal_type` column), repository upserts
    /// (`users.principal_type` CHECK constraint in
    /// `migrations/global/20260420120003_users.sql`), and test helpers
    /// (`src/auth/testing.rs`). One source of truth prevents the
    /// "serviceprincipal" vs "service_principal" drift that PR #106
    /// I1 flagged in `fire_admin_read_audit` / `fire_denied_audit`.
    pub fn as_canonical_str(self) -> &'static str {
        match self {
            PrincipalType::User => "user",
            PrincipalType::ServicePrincipal => "service_principal",
            PrincipalType::System => "system",
            PrincipalType::LegacyStatic => "legacy_static",
        }
    }
}

/// Extracted and validated per-request principal. Attached to request
/// extensions by the middleware. Handlers extract via the `AuthContext`
/// Axum extractor.
///
/// `Serialize + Deserialize` exists so `InboundMessage.auth_context`
/// round-trips cleanly through the per-message wire format. `#[serde(default)]`
/// at the `InboundMessage` level keeps older payloads (missing the field)
/// decoding to `None`, which falls through to `legacy_static()` at dispatch.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuthContext {
    pub principal_type: PrincipalType,
    /// Entra tenant ID (`tid` claim). Empty for `System` and `LegacyStatic`.
    pub tid: Arc<str>,
    /// Entra object ID (`oid` claim). Stable, tenant-wide, never UPN.
    /// Empty for `System` and `LegacyStatic`.
    pub oid: Arc<str>,
    /// App roles from the `roles` claim. Empty if unassigned.
    pub roles: Vec<Arc<str>>,
    /// Group object-IDs from the `groups` claim. Empty on overage
    /// (Phase 3 resolves via Graph).
    pub groups: Vec<Arc<str>>,
    /// True if the token indicated a groups overage and Graph lookup is
    /// needed to enumerate. Phase 3 populates.
    pub groups_overage: bool,
    /// Display-only email (`preferred_username` or `email` claim).
    /// Not suitable for authorization: the claim is mutable at the Entra
    /// side, so only the composite (tid, oid) is a stable identity key.
    pub display_email: Option<Arc<str>>,
    /// Display-only name. NEVER consulted for authorization.
    pub display_name: Option<Arc<str>>,
}

impl AuthContext {
    /// Construct the legacy-static context used when the `auth_token` branch
    /// authorizes a request.
    pub fn legacy_static() -> Self {
        Self {
            principal_type: PrincipalType::LegacyStatic,
            tid: Arc::from(""),
            oid: Arc::from(""),
            roles: Vec::new(),
            groups: Vec::new(),
            groups_overage: false,
            display_email: None,
            display_name: None,
        }
    }

    /// The principal key for audit logs and ownership columns: `{tid}:{oid}`
    /// for real Entra principals, `"legacy-static"` for the static-token
    /// branch, `"system"` for the Spacebot Cortex.
    pub fn principal_key(&self) -> String {
        match self.principal_type {
            PrincipalType::User | PrincipalType::ServicePrincipal => {
                format!("{}:{}", self.tid, self.oid)
            }
            PrincipalType::System => "system".to_string(),
            PrincipalType::LegacyStatic => "legacy-static".to_string(),
        }
    }

    /// True when the principal has the named app role.
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r.as_ref() == role)
    }
}

impl<S> FromRequestParts<S> for AuthContext
where
    S: Send + Sync,
{
    type Rejection = (axum::http::StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts.extensions.get::<AuthContext>().cloned().ok_or((
            axum::http::StatusCode::UNAUTHORIZED,
            "no auth context attached",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_static_principal_key_is_stable() {
        let ctx = AuthContext::legacy_static();
        assert_eq!(ctx.principal_key(), "legacy-static");
    }

    #[test]
    fn user_principal_key_is_tid_colon_oid() {
        let ctx = AuthContext {
            principal_type: PrincipalType::User,
            tid: Arc::from("ten-123"),
            oid: Arc::from("oid-abc"),
            roles: vec![],
            groups: vec![],
            groups_overage: false,
            display_email: None,
            display_name: None,
        };
        assert_eq!(ctx.principal_key(), "ten-123:oid-abc");
    }

    #[test]
    fn has_role_is_case_sensitive_and_matches_exact() {
        let ctx = AuthContext {
            principal_type: PrincipalType::User,
            tid: Arc::from("t"),
            oid: Arc::from("o"),
            roles: vec![Arc::from("SpacebotAdmin")],
            groups: vec![],
            groups_overage: false,
            display_email: None,
            display_name: None,
        };
        assert!(ctx.has_role("SpacebotAdmin"));
        assert!(!ctx.has_role("spacebotadmin"));
        assert!(!ctx.has_role("Admin"));
    }
}
