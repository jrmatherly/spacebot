//! Phase 6 Task 6.A.2 — unprotected `GET /api/auth/config` for SPA bootstrap.
//!
//! The browser fetches this before MSAL.js completes sign-in, so no bearer
//! token is available yet. Two auth middlewares allowlist this path (see
//! `src/auth/middleware.rs` for the Entra branch and `src/api/server.rs`
//! for the static-token branch).
//!
//! Payload whitelists exactly the three fields MSAL.js needs:
//!   - `client_id` — the SPA app registration (not the Web API registration)
//!   - `tenant_id` — used to compute the v2.0 authority URL
//!   - `authority` — pre-computed `https://login.microsoftonline.com/{tid}/v2.0`
//!   - `scopes` — the delegated scopes the SPA requests at sign-in
//!
//! When Entra is not configured, `entra_enabled: false` is returned and the
//! remaining fields are absent. The SPA reads that as "static-token mode"
//! and skips MSAL bootstrapping entirely.

use axum::Json;
use axum::extract::State;
use serde::Serialize;
use std::sync::Arc;

use super::state::ApiState;

/// SPA-safe MSAL bootstrap payload. Never contains secrets — the three
/// identifiers below are listed in the Entra tenant's OIDC discovery
/// document and the scope string is a public contract.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AuthConfigResponse {
    /// `true` when the daemon has a resolved `[api.auth.entra]` config. The
    /// SPA branches on this: `false` means fall back to static-token mode.
    pub entra_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<String>>,
}

/// `GET /api/auth/config` — unprotected endpoint that returns SPA bootstrap
/// values. Reaches the handler without an `Authorization` header because
/// both auth middlewares include this path in their allowlist (see module
/// doc comment).
#[utoipa::path(
    get,
    path = "/auth/config",
    responses((status = 200, description = "MSAL bootstrap config", body = AuthConfigResponse)),
    tag = "auth"
)]
pub(super) async fn get_auth_config(
    State(state): State<Arc<ApiState>>,
) -> Json<AuthConfigResponse> {
    let Some(cfg) = state.entra_auth_public_config() else {
        return Json(AuthConfigResponse {
            entra_enabled: false,
            client_id: None,
            tenant_id: None,
            authority: None,
            scopes: None,
        });
    };

    Json(AuthConfigResponse {
        entra_enabled: true,
        client_id: Some(cfg.spa_client_id.clone()),
        tenant_id: Some(cfg.tenant_id.clone()),
        authority: Some(format!(
            "https://login.microsoftonline.com/{}/v2.0",
            cfg.tenant_id
        )),
        scopes: Some(cfg.spa_scopes.clone()),
    })
}
