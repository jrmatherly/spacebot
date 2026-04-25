//! Thin wrapper around `jwt-authorizer` providing Entra-specific claim
//! extraction. The crate handles JWKS caching, OIDC discovery, and signature
//! verification. We layer on the Spacebot-specific validation rules.
//!
//! The [`JwtValidator`] trait abstracts over token validators so integration
//! tests can substitute a mock without running a real JWKS endpoint. The
//! companion [`DynJwtValidator`] trait (object-safe via `Pin<Box<dyn
//! Future>>`) is what `ApiState` holds, because `impl Future` in return
//! position is not object-safe. See `rust-patterns.md` § "Trait Design"
//! for the RPITIT + Dyn-companion pattern.

use crate::auth::config::EntraAuthConfig;
use crate::auth::context::{AuthContext, PrincipalType};
use crate::auth::errors::AuthError;

use jsonwebtoken::Algorithm;
use jwt_authorizer::{Authorizer, JwtAuthorizer, Refresh, RefreshStrategy, Validation};
use serde::Deserialize;

use std::time::Duration;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Validate an Entra-issued bearer token. Production uses [`EntraValidator`];
/// integration tests swap in `auth::testing::MockValidator` (Task 4.7).
///
/// Implementations return a fully-populated [`AuthContext`] on success and
/// an [`AuthError`] on failure. The enum's variants map to HTTP status codes
/// via [`AuthError::status`].
pub trait JwtValidator: Send + Sync {
    fn validate(&self, bearer: &str)
    -> impl Future<Output = Result<AuthContext, AuthError>> + Send;
}

/// Dyn-compatible companion to [`JwtValidator`]. Blanket impl forwards every
/// `JwtValidator` to a `Box<dyn Future>` return so `Arc<dyn DynJwtValidator>`
/// is object-safe. `ApiState.entra_auth` holds this variant.
pub trait DynJwtValidator: Send + Sync {
    fn validate_dyn<'a>(
        &'a self,
        bearer: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<AuthContext, AuthError>> + Send + 'a>>;
}

impl<T: JwtValidator + ?Sized> DynJwtValidator for T {
    fn validate_dyn<'a>(
        &'a self,
        bearer: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<AuthContext, AuthError>> + Send + 'a>> {
        Box::pin(<Self as JwtValidator>::validate(self, bearer))
    }
}

/// Claims extracted from Entra v2 tokens. Unused claims are ignored.
/// `_claim_names` / `_claim_sources` appear on groups-overage tokens. We
/// capture their presence here; Phase 3 resolves overage via
/// `GraphClient::list_member_groups` (see
/// `src/auth/middleware.rs::sync_groups_for_principal`).
#[derive(Debug, Clone, Deserialize)]
pub struct EntraClaims {
    /// Tenant ID.
    pub tid: String,
    /// Object ID. The stable identity key for authorization.
    pub oid: String,
    /// App roles. Absent entirely if the principal has none (not an empty array).
    #[serde(default)]
    pub roles: Vec<String>,
    /// Group object-IDs. Absent on overage (see `_claim_names`).
    #[serde(default)]
    pub groups: Vec<String>,
    /// Space-separated scopes (delegated tokens only).
    #[serde(default)]
    pub scp: Option<String>,
    /// Display-only. Mutable. Never used for authz.
    #[serde(default)]
    pub preferred_username: Option<String>,
    /// Display-only. Mutable. Never used for authz.
    #[serde(default)]
    pub email: Option<String>,
    /// Display-only. Mutable. Never used for authz.
    #[serde(default)]
    pub name: Option<String>,
    /// Indicates groups overage when present.
    #[serde(default, rename = "_claim_names")]
    pub claim_names: Option<serde_json::Value>,
}

/// Built from `EntraAuthConfig`. Held in `ApiState`.
///
/// Not `Clone`. Share via `Arc<EntraValidator>` when multiple callers need
/// access (middleware + admin handlers), which is how `ApiState` holds it.
pub struct EntraValidator {
    inner: Arc<Authorizer<EntraClaims>>,
    cfg: EntraAuthConfig,
}

impl EntraValidator {
    pub async fn new(cfg: EntraAuthConfig) -> anyhow::Result<Self> {
        if cfg.mock_mode {
            anyhow::bail!(
                "EntraValidator::new called with mock_mode=true; caller should \
                 use MockValidator instead (Phase 4 Task 4.7)"
            );
        }
        // `Algorithm::RS256` is imported from `jsonwebtoken` (not
        // `jwt_authorizer`) because jwt_authorizer does not re-export it.
        // Both crates are in [dependencies] for this reason.
        let aud_refs: Vec<&str> = vec![cfg.audience.as_ref()];
        let iss_string = cfg.issuer_override.clone().unwrap_or_else(|| cfg.issuer());
        let iss_refs: Vec<&str> = vec![iss_string.as_str()];
        // `nbf` validation is off by default in jwt-authorizer 0.15. Enable
        // it so not-yet-valid tokens are rejected (prevents tokens minted
        // with future `nbf` from being accepted ahead of schedule).
        let validation = Validation::new()
            .aud(&aud_refs)
            .iss(&iss_refs)
            .leeway(cfg.clock_skew_leeway_secs)
            .nbf(true)
            .algs(vec![Algorithm::RS256]);

        // Test override wins over computed URL so Wiremock-backed
        // integration tests can point the validator at a fake tenant.
        let jwks_url = cfg
            .jwks_url_override
            .clone()
            .unwrap_or_else(|| cfg.jwks_url());
        // SOC 2 / key-rotation correctness: jwt-authorizer's default Refresh
        // (refresh_interval = 600s, strategy = KeyNotFound) requires the
        // load time to exceed `refresh_interval` before the JWKS endpoint
        // is hit again on an unknown `kid`. That means a token signed with
        // a freshly-rotated Entra key is rejected with `InvalidKid` until
        // 10 minutes after the daemon's last successful JWKS fetch — for
        // up to 600s after every restart, every emergency rotation, etc.
        // Set `refresh_interval = 0` so unknown-kid triggers an immediate
        // refetch; `retry_interval` (10s default) still acts as a circuit
        // breaker against runaway loops if the JWKS endpoint is down.
        let refresh = Refresh {
            strategy: RefreshStrategy::KeyNotFound,
            refresh_interval: Duration::ZERO,
            retry_interval: Duration::from_secs(10),
        };
        let inner = JwtAuthorizer::<EntraClaims>::from_jwks_url(&jwks_url)
            .validation(validation)
            .refresh(refresh)
            .build()
            .await?;
        Ok(Self {
            inner: Arc::new(inner),
            cfg,
        })
    }

    /// Validate a raw bearer token string and produce an `AuthContext`.
    ///
    /// Also reachable via the [`JwtValidator`] trait so `ApiState.entra_auth`
    /// can hold any validator that implements it (mock or real).
    pub async fn validate(&self, bearer: &str) -> Result<AuthContext, AuthError> {
        let token_data = self
            .inner
            .check_auth(bearer)
            .await
            .map_err(map_authorizer_err)?;

        let claims = token_data.claims;

        // `scp` presence distinguishes delegated (User) from app-only
        // (ServicePrincipal). Matches Microsoft's guidance: `scp` only
        // appears on delegated tokens. `roles` can appear on either.
        let principal_type = if claims.scp.is_some() {
            PrincipalType::User
        } else {
            PrincipalType::ServicePrincipal
        };

        // Enforce scope requirement if configured.
        if !self.cfg.allowed_scopes.is_empty() {
            let token_scopes: Vec<&str> = claims
                .scp
                .as_deref()
                .unwrap_or("")
                .split_ascii_whitespace()
                .collect();
            let has_required = self
                .cfg
                .allowed_scopes
                .iter()
                .any(|required| token_scopes.contains(&required.as_str()));
            if !has_required && matches!(principal_type, PrincipalType::User) {
                return Err(AuthError::Forbidden("token lacks required scope".into()));
            }
            // For app-only / service-principal tokens (no `scp` claim), the
            // equivalent authz gate is REQUIRED role assignment. Otherwise
            // a service-principal token issued without specific app roles
            // for our app silently passes the scope gate. Enforce role
            // presence when allowed_scopes is configured.
            if matches!(principal_type, PrincipalType::ServicePrincipal) && claims.roles.is_empty()
            {
                return Err(AuthError::Forbidden(
                    "service principal token lacks any app roles".into(),
                ));
            }
        }

        let groups_overage = claims.claim_names.is_some();

        Ok(AuthContext {
            principal_type,
            tid: Arc::from(claims.tid.as_str()),
            oid: Arc::from(claims.oid.as_str()),
            roles: claims.roles.into_iter().map(Arc::from).collect(),
            groups: claims.groups.into_iter().map(Arc::from).collect(),
            groups_overage,
            display_email: claims.preferred_username.or(claims.email).map(Arc::from),
            display_name: claims.name.map(Arc::from),
        })
    }
}

impl JwtValidator for EntraValidator {
    fn validate(
        &self,
        bearer: &str,
    ) -> impl Future<Output = Result<AuthContext, AuthError>> + Send {
        // Delegate to the inherent method so callers using the concrete
        // type continue to work. The compiler erases this into the same
        // `impl Future` as `async fn validate(...)` above.
        Self::validate(self, bearer)
    }
}

fn map_authorizer_err(e: jwt_authorizer::error::AuthError) -> AuthError {
    use jsonwebtoken::errors::ErrorKind;
    use jwt_authorizer::error::AuthError as JE;
    // No `_ =>` wildcard: a jwt-authorizer minor-version variant must trigger
    // a compile-break so the auth surface is audited before rollout. Do not
    // add a catch-all arm.
    match e {
        JE::MissingToken() => AuthError::MissingHeader,
        // Split InvalidToken by inner jsonwebtoken ErrorKind so operators can
        // distinguish expired-token spam (retry hint) from bad-signature spam
        // (attack signal) via the `temporal_invalid` vs `invalid_token`
        // metric labels.
        JE::InvalidToken(err) => match err.kind() {
            ErrorKind::ExpiredSignature | ErrorKind::ImmatureSignature => {
                AuthError::TemporalInvalid
            }
            _ => AuthError::InvalidToken,
        },
        JE::InvalidKey(_) => AuthError::InvalidToken,
        JE::InvalidKeyAlg(_) => AuthError::InvalidToken,
        JE::InvalidClaims() => AuthError::InvalidToken,
        JE::JwksRefreshError(_) => AuthError::JwksUnreachable,
        JE::JwksSerialisationError(_) => AuthError::JwksUnreachable,
        JE::InvalidKid(_) => AuthError::InvalidToken,
        // Server-side misconfigurations surface as 503 rather than 401.
        // These are operator errors, not authentication failures. Returning
        // 401 would be an audit finding ("misconfig surfaced as user auth
        // failure").
        JE::NoAuthorizer() | JE::NoAuthorizerLayer() => AuthError::JwksUnreachable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::context::PrincipalType;
    use crate::auth::testing::{MockValidator, mint_mock_token};

    /// Regression guard for the `JwtValidator` / `DynJwtValidator` split.
    /// Asserts that `Arc<MockValidator>` unsize-coerces to
    /// `Arc<dyn DynJwtValidator>` via the blanket impl at
    /// `impl<T: JwtValidator + ?Sized> DynJwtValidator for T`, and that
    /// `validate_dyn` routes back to `JwtValidator::validate` identically
    /// to calling the concrete type directly.
    ///
    /// Production wires exactly this path at `src/main.rs` via
    /// `set_entra_auth(Arc::new(validator))`: the compiler does the
    /// coercion implicitly, and the middleware calls `.validate_dyn(...)`.
    /// Without this test, a regression in the blanket impl (e.g. a future
    /// `impl<T: SomeBound> JwtValidator for T` that breaks object safety)
    /// would only surface at production token-validation time.
    #[tokio::test]
    async fn dyn_jwt_validator_routes_through_blanket_impl() {
        use std::sync::Arc;

        let ctx = AuthContext {
            principal_type: PrincipalType::User,
            tid: Arc::from("t1"),
            oid: Arc::from("alice"),
            roles: vec![],
            groups: vec![],
            groups_overage: false,
            display_email: None,
            display_name: None,
        };
        let token = mint_mock_token(&ctx);

        // The coercion step this test exists to guard: concrete
        // Arc<MockValidator> -> Arc<dyn DynJwtValidator>.
        let concrete: Arc<MockValidator> = Arc::new(MockValidator::new());
        let erased: Arc<dyn DynJwtValidator> = concrete.clone();

        // Two independent validate calls: one through the concrete
        // inherent method, one through the dyn-erased trait object.
        // They must return identical AuthContext values.
        let direct = concrete.validate(&token).await.unwrap();
        let via_dyn = erased.validate_dyn(&token).await.unwrap();

        assert_eq!(direct.oid.as_ref(), via_dyn.oid.as_ref());
        assert_eq!(direct.tid.as_ref(), via_dyn.tid.as_ref());
        assert_eq!(direct.principal_type, via_dyn.principal_type);
    }
}
