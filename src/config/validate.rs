//! Cross-cutting deployment-invariant checks run after config load.
//!
//! These validate combinations of `[api]` settings + `SPACEBOT_DEPLOYMENT`
//! that individual field validation cannot catch on its own. Called once
//! from `main.rs` after the runtime `Config` is finalized.
//!
//! Rationale: Multi-team plan WS-1.3 (Hermes audit P1-2). A misconfigured
//! hosted deployment used to come up healthy with no auth backend; this
//! check fails the daemon at startup with a clear error message instead.

use crate::config::Config;
use crate::error::ConfigError;

/// Returns `true` when `SPACEBOT_DEPLOYMENT=hosted` (case-insensitive).
///
/// Mirrors the env-var read in `src/config/toml_schema.rs::hosted_api_bind`.
/// There is no parsed `Config.deployment_mode` field today; if a future
/// refactor introduces one, sweep both call sites at once.
fn is_hosted_deployment() -> bool {
    std::env::var("SPACEBOT_DEPLOYMENT")
        .ok()
        .map(|v| v.eq_ignore_ascii_case("hosted"))
        .unwrap_or(false)
}

/// Validate cross-cutting deployment invariants after config load.
///
/// Hard failures (return `Err`):
/// - Hosted mode without `[api.auth.entra]` configured.
/// - Hosted mode combined with `[api].allow_unauthenticated = true`.
///
/// Soft warning (logs `tracing::warn!` and returns `Ok`):
/// - `allow_unauthenticated = true` with no auth backend at all (no
///   `auth_token`, no Entra). Appropriate only for local dev or for
///   Envoy-SSO deployments where the cluster gateway guarantees no
///   unauthenticated request reaches this pod.
pub fn validate_deployment_invariants(cfg: &Config) -> Result<(), ConfigError> {
    let hosted = is_hosted_deployment();

    if hosted && cfg.api.entra_auth.is_none() {
        return Err(ConfigError::HostedRequiresEntra);
    }

    if hosted && cfg.api.allow_unauthenticated {
        return Err(ConfigError::AllowUnauthenticatedInHosted);
    }

    if cfg.api.allow_unauthenticated
        && cfg.api.auth_token.is_none()
        && cfg.api.entra_auth.is_none()
    {
        tracing::warn!(
            "API is configured to accept unauthenticated requests with no \
             auth backend. Appropriate only for local dev or Envoy-SSO \
             deployments where the cluster gateway guarantees no \
             unauthenticated request reaches this pod."
        );
    }

    Ok(())
}
