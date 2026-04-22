//! Test-only auth helpers: [`MockValidator`] + [`mint_mock_token`]. Compiled
//! unconditionally (NOT `#[cfg(test)]`) because integration tests under
//! `tests/*.rs` are separate compilation units that do not see `cfg(test)`
//! items from the library. Same precedent as `tests/support/mock_entra.rs`
//! and `ApiState::new_for_tests`.
//!
//! Not intended for production use. The validator accepts ANY bearer whose
//! body is a base64url-encoded JSON [`MintableAuthContext`]. Wiring this as
//! the validator in `ApiState.entra_auth` (via
//! `set_entra_auth(Arc::new(MockValidator::new()))`) bypasses JWKS entirely,
//! so the resulting deployment has no auth.
//!
//! The handler-integration tests in `tests/api_memories_authz.rs` use this
//! to inject specific `AuthContext` values without standing up a Wiremock
//! tenant + generating signed RS256 tokens.

use crate::auth::context::{AuthContext, PrincipalType};
use crate::auth::errors::AuthError;
use crate::auth::jwks::JwtValidator;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};

use std::sync::Arc;

/// Mock validator. Decodes the bearer as a base64url-encoded
/// [`MintableAuthContext`] and returns the embedded context verbatim.
#[derive(Debug, Default)]
pub struct MockValidator;

impl MockValidator {
    pub fn new() -> Self {
        Self
    }
}

impl JwtValidator for MockValidator {
    fn validate(
        &self,
        bearer: &str,
    ) -> impl std::future::Future<Output = Result<AuthContext, AuthError>> + Send {
        // Pure sync work wrapped in a ready future so the signature matches
        // the trait. Mocks don't hit any network, so there's no .await work.
        let parsed = URL_SAFE_NO_PAD
            .decode(bearer)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<MintableAuthContext>(&bytes).ok());
        async move {
            match parsed {
                Some(m) => Ok(m.into()),
                None => Err(AuthError::InvalidToken),
            }
        }
    }
}

/// Serializable wire form of [`AuthContext`]. Round-trips through the mock
/// token so integration tests can specify exactly which principal the
/// middleware sees.
#[derive(Debug, Serialize, Deserialize)]
struct MintableAuthContext {
    principal_type: String,
    tid: String,
    oid: String,
    #[serde(default)]
    roles: Vec<String>,
    #[serde(default)]
    groups: Vec<String>,
    #[serde(default)]
    display_email: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
}

impl From<MintableAuthContext> for AuthContext {
    fn from(m: MintableAuthContext) -> Self {
        let principal_type = match m.principal_type.as_str() {
            "user" => PrincipalType::User,
            "service_principal" => PrincipalType::ServicePrincipal,
            "system" => PrincipalType::System,
            _ => PrincipalType::LegacyStatic,
        };
        AuthContext {
            principal_type,
            tid: Arc::from(m.tid),
            oid: Arc::from(m.oid),
            roles: m.roles.into_iter().map(Arc::from).collect(),
            groups: m.groups.into_iter().map(Arc::from).collect(),
            groups_overage: false,
            display_email: m.display_email.map(Arc::from),
            display_name: m.display_name.map(Arc::from),
        }
    }
}

/// Mint a mock bearer token encoding the given context. Round-trips back
/// through [`MockValidator::validate`] into an identical [`AuthContext`].
pub fn mint_mock_token(ctx: &AuthContext) -> String {
    let mintable = MintableAuthContext {
        principal_type: match ctx.principal_type {
            PrincipalType::User => "user".into(),
            PrincipalType::ServicePrincipal => "service_principal".into(),
            PrincipalType::System => "system".into(),
            PrincipalType::LegacyStatic => "legacy_static".into(),
        },
        tid: ctx.tid.to_string(),
        oid: ctx.oid.to_string(),
        roles: ctx.roles.iter().map(|r| r.to_string()).collect(),
        groups: ctx.groups.iter().map(|g| g.to_string()).collect(),
        display_email: ctx.display_email.as_deref().map(|s| s.to_string()),
        display_name: ctx.display_name.as_deref().map(|s| s.to_string()),
    };
    let json = serde_json::to_vec(&mintable).expect("MintableAuthContext serialize");
    URL_SAFE_NO_PAD.encode(json)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alice() -> AuthContext {
        AuthContext {
            principal_type: PrincipalType::User,
            tid: Arc::from("t1"),
            oid: Arc::from("alice"),
            roles: vec![Arc::from("SpacebotUser")],
            groups: vec![],
            groups_overage: false,
            display_email: Some(Arc::from("alice@example.com")),
            display_name: Some(Arc::from("Alice")),
        }
    }

    #[tokio::test]
    async fn mock_round_trips_auth_context() {
        let ctx = alice();
        let token = mint_mock_token(&ctx);
        let validator = MockValidator::new();
        let recovered = validator.validate(&token).await.unwrap();
        assert_eq!(recovered.tid.as_ref(), "t1");
        assert_eq!(recovered.oid.as_ref(), "alice");
        assert!(matches!(recovered.principal_type, PrincipalType::User));
        assert_eq!(recovered.roles.len(), 1);
        assert_eq!(recovered.roles[0].as_ref(), "SpacebotUser");
    }

    #[tokio::test]
    async fn mock_rejects_garbage() {
        let validator = MockValidator::new();
        assert!(validator.validate("not-a-token").await.is_err());
        assert!(validator.validate("").await.is_err());
    }
}
