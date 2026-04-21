//! Per-request auth context. Populated by the JWT or static-token middleware
//! and consumed by per-handler authz helpers in Phase 4+.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use std::sync::Arc;

/// Stable identifier category for the authenticated principal.
/// See research §12 A-5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
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

/// Extracted and validated per-request principal. Attached to request
/// extensions by the middleware; extracted by handlers via the
/// `AuthContext` Axum extractor.
#[derive(Debug, Clone)]
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
    /// NEVER consulted for authorization. See §12 E-7.
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
