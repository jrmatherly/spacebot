//! Shared path-allowlist for the auth middleware branches.
//!
//! Two middleware implementations (`src/api/server.rs`
//! `api_auth_middleware` for static-token deployments and
//! `src/auth/middleware.rs` `entra_auth_middleware` for Entra JWT
//! deployments) each run BEFORE the router dispatches. Some endpoints
//! must reach the handler without an `Authorization` header:
//!
//!   - `/health` + `/api/health` — the daemon liveness probe is polled
//!     by container orchestrators that have no bearer token.
//!   - `/api/auth/config` (Phase 6) — the SPA's MSAL bootstrap fetch
//!     runs before sign-in completes, so it has no token yet.
//!   - `/api/desktop/tokens` (Phase 8) — the Tauri desktop loopback
//!     listener posts tokens it just acquired via system-browser SSO;
//!     by definition no bearer token is available yet. Transport-level
//!     protection (peer IP is_loopback + Host header pin) lives in the
//!     handler, not this allowlist.
//!
//! Historically each middleware hand-maintained the allowlist as a
//! local `if path == "..." || path == "..."` block. That worked for a
//! single entry (`/health` + its `/api/` variant) but broke down when
//! Phase 6 added `/api/auth/config`: the literal had to be copied into
//! two places, and a future contributor adding a third entry to one
//! branch but forgetting the other would produce a silent security
//! asymmetry.
//!
//! This module centralizes the list in one place. Both middleware
//! branches call [`is_auth_bypassed`] against the request path.

/// Paths that bypass the auth middleware in both static-token and
/// Entra-JWT deployments. Kept as a `&[&str]` so the compile-time
/// sort/uniqueness test below can inspect it without `Vec`-allocating.
///
/// When adding a new entry: verify the handler it routes to is genuinely
/// safe to expose without auth, add a regression test against BOTH
/// middleware branches (see `tests/api_auth_middleware.rs` and the
/// `router_level` mod in `tests/entra_jwt_middleware.rs`), and document
/// the rationale here.
pub(crate) const AUTH_BYPASS_PATHS: &[&str] = &[
    "/api/auth/config",
    "/api/desktop/tokens",
    "/api/health",
    "/health",
];

/// Returns true when the request path bypasses the auth middleware.
/// Exact-match only; prefix matching is deliberately NOT supported so a
/// hypothetical `/api/health/leak` handler cannot inherit the bypass.
#[inline]
pub(crate) fn is_auth_bypassed(path: &str) -> bool {
    AUTH_BYPASS_PATHS.contains(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Enforces the documented invariant that the allowlist is sorted
    /// and unique. Adding an entry that breaks the order fails this
    /// test, which catches the kind of drift an editor's auto-format
    /// might introduce.
    #[test]
    fn allowlist_is_sorted_and_unique() {
        for window in AUTH_BYPASS_PATHS.windows(2) {
            assert!(
                window[0] < window[1],
                "AUTH_BYPASS_PATHS must be sorted ascending and unique: \
                 found {:?} before {:?}",
                window[0],
                window[1],
            );
        }
    }

    #[test]
    fn bypass_matches_documented_entries() {
        assert!(is_auth_bypassed("/health"));
        assert!(is_auth_bypassed("/api/health"));
        assert!(is_auth_bypassed("/api/auth/config"));
        assert!(is_auth_bypassed("/api/desktop/tokens"));
    }

    #[test]
    fn bypass_is_exact_match_only() {
        // Prefix-matching would be a latent security hole.
        assert!(!is_auth_bypassed("/api/auth/config/leak"));
        assert!(!is_auth_bypassed("/api/health/status"));
        assert!(!is_auth_bypassed("/healthcheck"));
        assert!(!is_auth_bypassed(""));
        assert!(!is_auth_bypassed("/"));
    }
}
