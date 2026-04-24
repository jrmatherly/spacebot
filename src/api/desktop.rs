//! `POST /api/desktop/tokens` — loopback-only ingestion of Entra tokens
//! the Tauri desktop app acquired via system-browser SSO.
//!
//! This endpoint bypasses the auth middleware (registered in
//! `src/auth/bypass.rs`) because the desktop has no bearer token at call
//! time; the JWT it is delivering is exactly what will unlock future
//! authenticated requests. Protection is therefore transport-level, not
//! middleware-level, enforced three ways:
//!
//!   1. Peer IP must satisfy `is_loopback()` (rejects any non-127.0.0.1 /
//!      non-::1 connection, including bridged container traffic).
//!   2. `Host` header must resolve to `127.0.0.1`, `[::1]`, or `localhost`
//!      (defends against DNS-rebinding attacks where an attacker-controlled
//!      name resolves to 127.0.0.1 in the victim's browser).
//!   3. Tokens land in `SecretCategory::System`, which the daemon's secret
//!      store refuses to persist when locked — surfacing a distinct
//!      `SERVICE_UNAVAILABLE` the Tauri side translates into a user-facing
//!      unlock prompt.

use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::StatusCode;
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::Arc;

use super::state::ApiState;
use crate::secrets::store::SecretCategory;

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct DesktopTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    // The Tauri side sends this alongside the tokens; the daemon will
    // eventually stash it for refresh-deadline tracking, at which point
    // the allow goes away.
    #[allow(dead_code)]
    pub expires_in: u64,
}

/// Accept tokens acquired by the Tauri desktop app's loopback auth flow
/// and persist them via the daemon's `SecretsStore`.
#[utoipa::path(
    post,
    path = "/desktop/tokens",
    request_body = DesktopTokens,
    responses(
        (status = 204, description = "Tokens stored"),
        (status = 403, description = "Request not from loopback"),
        (status = 503, description = "Daemon secret store is locked"),
        (status = 500, description = "Secret store write failed"),
    ),
    tag = "desktop"
)]
pub(super) async fn store_desktop_tokens(
    State(state): State<Arc<ApiState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(tokens): Json<DesktopTokens>,
) -> Result<StatusCode, StatusCode> {
    if !peer.ip().is_loopback() {
        tracing::warn!(
            peer_ip = %peer.ip(),
            "rejected /api/desktop/tokens from non-loopback peer"
        );
        return Err(StatusCode::FORBIDDEN);
    }

    // Defend against DNS-rebinding: a malicious page could ask the
    // browser to resolve attacker.example to 127.0.0.1 and post to this
    // endpoint. The peer-IP check passes because the browser connects
    // to 127.0.0.1 locally, but the `Host` header still carries the
    // attacker's chosen name. Entra's loopback docs require the redirect
    // URI be pinned to `127.0.0.1` specifically, so we pin the inbound
    // `Host` to the same set.
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let host_name = host.split(':').next().unwrap_or("");
    if !matches!(host_name, "127.0.0.1" | "[::1]" | "localhost") {
        tracing::warn!(host = %host, "/api/desktop/tokens rejected non-loopback Host");
        return Err(StatusCode::FORBIDDEN);
    }

    let secrets = state
        .secrets_store
        .load()
        .as_ref()
        .as_ref()
        .cloned()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Err(e) = secrets.set(
        "entra_access_token",
        &tokens.access_token,
        SecretCategory::System,
    ) {
        // `SecretsError` does not currently expose a `Locked` arm as a
        // named public variant. The Display impl at
        // src/secrets/store.rs:122 writes the literal `"locked"` for
        // StoreState::Locked, so substring-match is the cheapest
        // discriminator. If the phrase shifts, the fallback arm returns
        // 500 — a locked store would surface as 500 instead of 503 until
        // this gets updated.
        if format!("{e}").contains("locked") {
            tracing::warn!("desktop token store rejected: daemon is locked");
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }
        tracing::error!(?e, "secrets.set failed for entra_access_token");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    if let Some(rt) = tokens.refresh_token
        && let Err(e) = secrets.set("entra_refresh_token", &rt, SecretCategory::System)
    {
        if format!("{e}").contains("locked") {
            tracing::warn!("desktop token store rejected: daemon is locked");
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }
        tracing::error!(?e, "secrets.set failed for entra_refresh_token");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(StatusCode::NO_CONTENT)
}
