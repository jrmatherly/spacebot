//! App role constants (mirrored from the Entra app registration) and the
//! `require_role` helper used by handlers. The constants must match the
//! `displayName` / `value` pairs in the Web API app registration manifest
//! at `docs/design-docs/entra-app-registrations.md`.

use crate::auth::context::{AuthContext, PrincipalType};
use crate::auth::errors::AuthError;

pub const ROLE_ADMIN: &str = "SpacebotAdmin";
pub const ROLE_USER: &str = "SpacebotUser";
pub const ROLE_SERVICE: &str = "SpacebotService";

/// Returns `Ok(())` if the principal holds the role, or if it's a principal
/// that bypasses role checks (`LegacyStatic` for backward compat, `System`
/// for cortex-initiated operations). Returns `AuthError::Forbidden` otherwise.
pub fn require_role(ctx: &AuthContext, role: &str) -> Result<(), AuthError> {
    if matches!(
        ctx.principal_type,
        PrincipalType::LegacyStatic | PrincipalType::System
    ) {
        return Ok(());
    }
    if ctx.has_role(role) {
        Ok(())
    } else {
        Err(AuthError::Forbidden(format!("requires role {role}")))
    }
}

/// True when the principal bypasses per-resource ownership checks. The
/// matrix in `docs/design-docs/entra-role-permission-matrix.md` documents
/// which resource-action cells are admin-only; this helper answers the
/// cross-cutting "can this principal always proceed" question.
pub fn is_admin(ctx: &AuthContext) -> bool {
    matches!(
        ctx.principal_type,
        PrincipalType::LegacyStatic | PrincipalType::System
    ) || ctx.has_role(ROLE_ADMIN)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn user_ctx(roles: Vec<&str>) -> AuthContext {
        AuthContext {
            principal_type: PrincipalType::User,
            tid: Arc::from("t1"),
            oid: Arc::from("u1"),
            roles: roles.into_iter().map(Arc::from).collect(),
            groups: vec![],
            groups_overage: false,
            display_email: None,
            display_name: None,
        }
    }

    #[test]
    fn require_role_allows_matching_role() {
        let ctx = user_ctx(vec![ROLE_USER]);
        assert!(require_role(&ctx, ROLE_USER).is_ok());
    }

    #[test]
    fn require_role_rejects_missing_role() {
        let ctx = user_ctx(vec![ROLE_USER]);
        let err = require_role(&ctx, ROLE_ADMIN).unwrap_err();
        assert!(matches!(err, AuthError::Forbidden(_)));
    }

    #[test]
    fn require_role_bypasses_for_legacy_static() {
        let ctx = AuthContext::legacy_static();
        assert!(require_role(&ctx, ROLE_ADMIN).is_ok());
    }

    #[test]
    fn is_admin_respects_role_claim() {
        assert!(is_admin(&user_ctx(vec![ROLE_ADMIN])));
        assert!(!is_admin(&user_ctx(vec![ROLE_USER])));
    }

    #[test]
    fn is_admin_bypasses_for_legacy_static() {
        assert!(is_admin(&AuthContext::legacy_static()));
    }
}
