//! Runtime config for Entra JWT validation. Loaded once at startup. Hot
//! reload is deferred. Changes require daemon restart.
//!
//! The raw TOML form lives in `src/config/toml_schema.rs` as
//! `TomlEntraAuthConfig`. `load.rs` resolves `secret:…` / `env:…`
//! references and constructs this struct.

use std::sync::Arc;

/// Resolved Entra auth config.
#[derive(Debug, Clone)]
pub struct EntraAuthConfig {
    /// Entra tenant ID (GUID).
    pub tenant_id: Arc<str>,
    /// Expected `aud` claim value. For v2.0 tokens this is the Web API
    /// registration's **client ID GUID**, not the Application ID URI.
    /// Requires `accessTokenAcceptedVersion: 2` in the app manifest.
    pub audience: Arc<str>,
    /// Required scopes (`scp` claim, space-separated). Typically `["api.access"]`.
    /// If empty, `scp` presence is not checked (app-only tokens are allowed).
    pub allowed_scopes: Vec<String>,
    /// How long to cache JWKS before re-fetching. Microsoft recommends 24h
    /// max. Entra rotates signing keys roughly weekly.
    pub jwks_cache_ttl_secs: u64,
    /// Clock-skew tolerance in seconds. Default 60. Never exceed 300.
    pub clock_skew_leeway_secs: u64,
    /// Graph group-membership cache TTL. Read by Phase 3's
    /// `sync_groups_for_principal`. Default 300 seconds.
    pub group_cache_ttl_secs: u64,
    /// SPA app registration's client ID GUID. Returned via `/api/auth/config`
    /// to the browser SPA. Never a secret. Distinct from `audience` (which
    /// is the Web API registration's client ID).
    pub spa_client_id: Arc<str>,
    /// Scopes the SPA requests during sign-in. Typically
    /// `["api://{web-api-guid}/api.access"]`. Returned via `/api/auth/config`.
    pub spa_scopes: Vec<Arc<str>>,
    /// Mock mode for local dev / CI. When true, tokens are "validated" by
    /// accepting any JWT with `aud`, `tid`, `oid` claims without signature
    /// check. The mock validator itself is not shipped yet; setting this
    /// true is rejected at daemon startup.
    pub mock_mode: bool,
    /// Test-only override for the computed JWKS URL. Not settable outside the
    /// `auth` module by construction. Integration tests use
    /// `EntraAuthConfig::new_test_with_overrides` below; the production loader
    /// always leaves this `None`.
    pub(crate) jwks_url_override: Option<String>,
    /// Test-only override for the issuer claim validator. Paired with
    /// `jwks_url_override`.
    pub(crate) issuer_override: Option<String>,
}

impl EntraAuthConfig {
    /// Compute the OIDC issuer URL for the configured tenant.
    /// Always v2.0 endpoint.
    pub fn issuer(&self) -> String {
        format!("https://login.microsoftonline.com/{}/v2.0", self.tenant_id)
    }

    /// Compute the OIDC discovery document URL.
    pub fn discovery_url(&self) -> String {
        format!(
            "https://login.microsoftonline.com/{}/v2.0/.well-known/openid-configuration",
            self.tenant_id
        )
    }

    /// Compute the JWKS URL directly (bypasses discovery for single-tenant).
    pub fn jwks_url(&self) -> String {
        format!(
            "https://login.microsoftonline.com/{}/discovery/v2.0/keys",
            self.tenant_id
        )
    }

    /// Integration-test constructor. Always-compiled (`#[doc(hidden)]`) so
    /// tests under `tests/*.rs`, which compile as separate crates, can build
    /// a config without naming the `pub(crate)` override fields. Paired with
    /// [`Self::with_test_overrides`].
    #[doc(hidden)]
    pub fn new_for_test(
        tenant_id: Arc<str>,
        audience: Arc<str>,
        allowed_scopes: Vec<String>,
        spa_client_id: Arc<str>,
        spa_scopes: Vec<Arc<str>>,
    ) -> Self {
        Self {
            tenant_id,
            audience,
            allowed_scopes,
            jwks_cache_ttl_secs: 3600,
            clock_skew_leeway_secs: 60,
            group_cache_ttl_secs: 300,
            spa_client_id,
            spa_scopes,
            mock_mode: false,
            jwks_url_override: None,
            issuer_override: None,
        }
    }

    /// Integration-test helper that injects Wiremock-backed JWKS and issuer
    /// URLs. The override fields are `pub(crate)` so no other code path can
    /// set them.
    #[doc(hidden)]
    pub fn with_test_overrides(mut self, jwks_url: String, issuer: String) -> Self {
        self.jwks_url_override = Some(jwks_url);
        self.issuer_override = Some(issuer);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_cfg() -> EntraAuthConfig {
        EntraAuthConfig {
            tenant_id: Arc::from("00000000-0000-0000-0000-000000000001"),
            audience: Arc::from("api://test"),
            allowed_scopes: vec!["api.access".into()],
            jwks_cache_ttl_secs: 3600,
            clock_skew_leeway_secs: 60,
            group_cache_ttl_secs: 300,
            spa_client_id: Arc::from("22222222-2222-2222-2222-222222222222"),
            spa_scopes: vec![Arc::from("api://test/api.access")],
            mock_mode: false,
            jwks_url_override: None,
            issuer_override: None,
        }
    }

    #[test]
    fn issuer_is_v2_endpoint() {
        let cfg = sample_cfg();
        assert_eq!(
            cfg.issuer(),
            "https://login.microsoftonline.com/00000000-0000-0000-0000-000000000001/v2.0"
        );
    }

    #[test]
    fn discovery_and_jwks_urls_are_microsoft_hosts() {
        let cfg = sample_cfg();
        assert!(
            cfg.discovery_url()
                .contains("/v2.0/.well-known/openid-configuration")
        );
        assert!(cfg.jwks_url().contains("/discovery/v2.0/keys"));
    }
}
