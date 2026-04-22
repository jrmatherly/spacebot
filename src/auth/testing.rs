//! Test-only auth helpers: [`MockValidator`] + [`mint_mock_token`]. Compiled
//! unconditionally (NOT `#[cfg(test)]`) because integration tests under
//! `tests/*.rs` are separate compilation units that do not see `cfg(test)`
//! items from the library. Same precedent as `tests/support/mock_entra.rs`
//! and `ApiState::new_for_tests`.
//!
//! Not intended for production use. The validator accepts ANY bearer whose
//! body is a base64url-encoded JSON [`MintableAuthContext`]. Wiring this as
//! the validator in `ApiState.entra_auth` bypasses JWKS entirely, so the
//! resulting deployment has no auth. Typical install:
//!
//! ```ignore
//! let mock: std::sync::Arc<dyn crate::auth::jwks::DynJwtValidator> =
//!     std::sync::Arc::new(MockValidator::new());
//! state.entra_auth.store(std::sync::Arc::new(Some(mock)));
//! ```
//!
//! The explicit trait-object cast is necessary because
//! `ApiState.entra_auth` holds `Arc<dyn DynJwtValidator>` and the compiler
//! will not infer the coercion from the bare `Arc<MockValidator>`.
//!
//! ## Parity with [`crate::auth::EntraValidator`]
//!
//! The mock does NOT enforce the production validator's semantic gates:
//!
//! - No `scp` presence check -> `User` vs `ServicePrincipal` (the caller
//!   sets `principal_type` directly in the minted token).
//! - No `allowed_scopes` enforcement for delegated tokens.
//! - No required-role enforcement for service-principal tokens.
//! - No `claim_names` -> `groups_overage` inference (the field is
//!   round-tripped verbatim).
//!
//! Tests are responsible for minting `AuthContext` values that the real
//! validator could plausibly have produced. A test that passes with a
//! `ServicePrincipal` + empty roles is not a valid production path; the
//! real validator rejects that shape at
//! `src/auth/jwks.rs`'s role-gate check.
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
        //
        // Three distinct failure modes are split so integration tests can
        // diagnose fixture drift from the `tracing::debug!` output:
        //   1. base64url decode failure
        //   2. JSON parse failure
        //   3. unknown principal_type discriminator (valid JSON, bad shape)
        // All three collapse to `AuthError::InvalidToken` for the middleware
        // (which maps to 401), but the debug logs preserve the signal.
        let parsed: Result<AuthContext, AuthError> = match URL_SAFE_NO_PAD.decode(bearer) {
            Err(e) => {
                tracing::debug!(%e, "mock token rejected: base64url decode failed");
                Err(AuthError::InvalidToken)
            }
            Ok(bytes) => match serde_json::from_slice::<MintableAuthContext>(&bytes) {
                Err(e) => {
                    tracing::debug!(%e, "mock token rejected: MintableAuthContext JSON parse failed");
                    Err(AuthError::InvalidToken)
                }
                Ok(m) => AuthContext::try_from(m).map_err(|e| {
                    tracing::debug!(%e, "mock token rejected: unknown principal_type");
                    AuthError::InvalidToken
                }),
            },
        };
        async move { parsed }
    }
}

/// Serializable wire form of [`AuthContext`]. Round-trips through the mock
/// token so integration tests can specify exactly which principal the
/// middleware sees.
///
/// `groups_overage` is included explicitly so the round-trip is lossless.
/// Tests exercising the Phase 3 overage path (where the token indicates
/// `_claim_names` and Graph must be consulted) need this flag to survive
/// the serialization boundary.
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
    groups_overage: bool,
    #[serde(default)]
    display_email: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
}

/// Error variants for `MintableAuthContext -> AuthContext` conversion.
/// Surfaced as `AuthError::InvalidToken` at the validator boundary but
/// named here so the debug log can distinguish fixture mistakes.
#[derive(Debug, thiserror::Error)]
enum MintableParseError {
    #[error("unknown principal_type discriminator: {0:?}")]
    UnknownPrincipalType(String),
}

impl TryFrom<MintableAuthContext> for AuthContext {
    type Error = MintableParseError;

    fn try_from(m: MintableAuthContext) -> Result<Self, Self::Error> {
        // Explicit match without a catch-all arm: a typo in a test's
        // `principal_type` field previously silently downgraded the token
        // to `LegacyStatic`, which sits in the admin-bypass set, producing
        // a false admin principal instead of a rejected token.
        let principal_type = match m.principal_type.as_str() {
            "user" => PrincipalType::User,
            "service_principal" => PrincipalType::ServicePrincipal,
            "system" => PrincipalType::System,
            "legacy_static" => PrincipalType::LegacyStatic,
            other => {
                return Err(MintableParseError::UnknownPrincipalType(other.to_string()));
            }
        };
        Ok(AuthContext {
            principal_type,
            tid: Arc::from(m.tid),
            oid: Arc::from(m.oid),
            roles: m.roles.into_iter().map(Arc::from).collect(),
            groups: m.groups.into_iter().map(Arc::from).collect(),
            groups_overage: m.groups_overage,
            display_email: m.display_email.map(Arc::from),
            display_name: m.display_name.map(Arc::from),
        })
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
        groups_overage: ctx.groups_overage,
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

    #[tokio::test]
    async fn round_trip_preserves_groups_overage() {
        // Regression guard for PR #104 review finding I9: overage flag
        // was previously hardcoded to false on parse, losing the signal
        // for any test that needs to exercise the Phase 3 Graph lookup
        // path triggered by an overage token.
        let mut ctx = alice();
        ctx.groups_overage = true;
        let token = mint_mock_token(&ctx);
        let validator = MockValidator::new();
        let recovered = validator.validate(&token).await.unwrap();
        assert!(
            recovered.groups_overage,
            "groups_overage must survive the mint/validate round trip"
        );
    }

    #[tokio::test]
    async fn round_trip_preserves_all_principal_types() {
        // Regression guard for PR #104 review finding I8: the previous
        // catch-all `_ => LegacyStatic` silently downgraded typos to the
        // admin-bypass principal. Now every principal_type MUST survive
        // the round trip; unknown discriminators reject.
        for pt in [
            PrincipalType::User,
            PrincipalType::ServicePrincipal,
            PrincipalType::System,
            PrincipalType::LegacyStatic,
        ] {
            let mut ctx = alice();
            ctx.principal_type = pt;
            let token = mint_mock_token(&ctx);
            let validator = MockValidator::new();
            let recovered = validator.validate(&token).await.unwrap();
            assert_eq!(
                recovered.principal_type, pt,
                "principal_type must round-trip faithfully: {:?}",
                pt
            );
        }
    }

    #[tokio::test]
    async fn mock_rejects_unknown_principal_type() {
        // Directly craft a MintableAuthContext with an invalid
        // principal_type, serialize + encode it, and confirm the mock
        // rejects it instead of silently downgrading to LegacyStatic.
        let bad_json = serde_json::json!({
            "principal_type": "not_a_real_principal_type",
            "tid": "t1",
            "oid": "alice",
        });
        let bytes = serde_json::to_vec(&bad_json).unwrap();
        let token = URL_SAFE_NO_PAD.encode(bytes);
        let validator = MockValidator::new();
        let result = validator.validate(&token).await;
        assert!(
            matches!(result, Err(AuthError::InvalidToken)),
            "unknown principal_type must reject, not default to LegacyStatic"
        );
    }
}
