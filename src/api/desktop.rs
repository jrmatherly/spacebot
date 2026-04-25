//! Loopback-only Entra token endpoints for the Tauri desktop app.
//!
//! Three verbs share the same path and the same defenses:
//!
//!   * `POST /api/desktop/tokens` — ingest tokens after the desktop
//!     completes the system-browser sign-in flow.
//!   * `GET /api/desktop/tokens` — return the cached access token (or
//!     `null`) for cold-start sessions that already authenticated in a
//!     prior run.
//!   * `DELETE /api/desktop/tokens` — wipe both access and refresh
//!     tokens on sign-out.
//!
//! All three bypass the auth middleware (registered in
//! `src/auth/bypass.rs`) because the desktop has no bearer token at call
//! time. The JWT being delivered (POST) or read (GET) is exactly what
//! will unlock future authenticated requests. Protection is therefore
//! transport-level, not middleware-level, enforced three ways:
//!
//!   1. Peer IP must satisfy `is_loopback()` (rejects any non-127.0.0.1 /
//!      non-::1 connection, including bridged container traffic).
//!   2. `Host` header must resolve to `127.0.0.1`, `[::1]`, or `localhost`
//!      (defends against DNS-rebinding attacks where an attacker-controlled
//!      name resolves to 127.0.0.1 in the victim's browser).
//!   3. Tokens land in `SecretCategory::System`, which the daemon's secret
//!      store refuses to persist when locked. That surfaces a distinct
//!      `SERVICE_UNAVAILABLE` the Tauri side translates into a user-facing
//!      unlock prompt.

use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;

use super::state::ApiState;
use crate::error::SecretsError;
use crate::secrets::store::{SecretCategory, SecretsStore};

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct DesktopTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    // TODO: persist for refresh-deadline tracking once a refresh
    // scheduler exists. Until then, the value is accepted off the wire
    // but not stored.
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
    enforce_loopback_preconditions(&peer, &headers)?;

    let secrets = state
        .secrets_store
        .load()
        .as_ref()
        .as_ref()
        .cloned()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let has_refresh = tokens.refresh_token.is_some();

    classify_secret_write(
        secrets.set(
            "entra_access_token",
            &tokens.access_token,
            SecretCategory::System,
        ),
        "entra_access_token",
    )?;

    if let Some(rt) = tokens.refresh_token
        && let Err(status) = classify_secret_write(
            secrets.set("entra_refresh_token", &rt, SecretCategory::System),
            "entra_refresh_token",
        )
    {
        // Access-token write already succeeded. Roll it back so the
        // store doesn't carry a stranded access token whose paired
        // refresh token never landed. Failure to roll back is logged
        // but cannot be surfaced to the caller (the original status
        // is what they need to act on).
        rollback_access_token(&secrets);
        return Err(status);
    }

    tracing::info!(
        peer_ip = %peer.ip(),
        has_refresh_token = has_refresh,
        expires_in = tokens.expires_in,
        "desktop sign-in tokens stored via /api/desktop/tokens"
    );
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct DesktopTokenStatus {
    pub access_token: Option<String>,
}

/// Headers attached to every `DesktopTokenStatus` response.
///
/// `no-store, no-cache` blocks any HTTP cache (Tauri WebView's, a
/// transparent local proxy, a future browser-served deployment) from
/// holding the JWT response body. The endpoint is called on every
/// cold start, so a single misconfigured cache could persist a token
/// far longer than the daemon intends.
fn no_store_headers() -> axum::http::HeaderMap {
    let mut h = axum::http::HeaderMap::new();
    h.insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store, no-cache"),
    );
    h
}

/// Return the cached access token persisted by an earlier
/// `store_desktop_tokens` call, or `null` if none exists.
///
/// Same loopback + Host defenses as the POST sibling. The Tauri shim
/// uses this on cold start to seed an `AccountInfo` into MSAL without
/// re-running the system-browser sign-in flow. Response carries
/// `Cache-Control: no-store` so the JWT is never cached.
#[utoipa::path(
    get,
    path = "/desktop/tokens",
    responses(
        (status = 200, description = "Cached access token (may be null)", body = DesktopTokenStatus),
        (status = 403, description = "Request not from loopback"),
        (status = 503, description = "Daemon secret store is locked"),
        (status = 500, description = "Secret store read failed"),
    ),
    tag = "desktop"
)]
pub(super) async fn get_desktop_tokens(
    State(state): State<Arc<ApiState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
) -> Result<(axum::http::HeaderMap, Json<DesktopTokenStatus>), StatusCode> {
    enforce_loopback_preconditions(&peer, &headers)?;

    let secrets = state
        .secrets_store
        .load()
        .as_ref()
        .as_ref()
        .cloned()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    match secrets.get("entra_access_token") {
        Ok(decrypted) => Ok((
            no_store_headers(),
            Json(DesktopTokenStatus {
                access_token: Some(decrypted.expose().to_string()),
            }),
        )),
        Err(SecretsError::StoreLocked) => {
            tracing::warn!("desktop token read rejected: daemon is locked");
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
        Err(SecretsError::NotFound { .. }) => Ok((
            no_store_headers(),
            Json(DesktopTokenStatus {
                access_token: None,
            }),
        )),
        Err(e) => {
            tracing::error!(error = %e, "secrets.get failed for entra_access_token");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Wipe both `entra_access_token` and `entra_refresh_token` from the
/// daemon's secret store. Idempotent: returns 204 whether or not the
/// keys were present.
#[utoipa::path(
    delete,
    path = "/desktop/tokens",
    responses(
        (status = 204, description = "Tokens cleared (or already absent)"),
        (status = 403, description = "Request not from loopback"),
        (status = 503, description = "Daemon secret store is locked"),
        (status = 500, description = "Secret store delete failed"),
    ),
    tag = "desktop"
)]
pub(super) async fn delete_desktop_tokens(
    State(state): State<Arc<ApiState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
) -> Result<StatusCode, StatusCode> {
    enforce_loopback_preconditions(&peer, &headers)?;

    let secrets = state
        .secrets_store
        .load()
        .as_ref()
        .as_ref()
        .cloned()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // `SecretsStore::delete` itself returns `Err(StoreLocked)` on a
    // locked store, so the same `classify_secret_delete` mapper that
    // handles per-key NotFound / Other cases also surfaces 503 to the
    // caller. No pre-check needed.
    classify_secret_delete(secrets.delete("entra_access_token"), "entra_access_token")?;
    classify_secret_delete(secrets.delete("entra_refresh_token"), "entra_refresh_token")?;

    tracing::info!(
        peer_ip = %peer.ip(),
        "desktop tokens cleared via /api/desktop/tokens"
    );
    Ok(StatusCode::NO_CONTENT)
}

/// Shared loopback defenses for the POST/GET/DELETE verbs on
/// `/api/desktop/tokens`. Returns `Err(StatusCode::FORBIDDEN)` on
/// either failure with a structured warn-level event naming what the
/// caller violated.
fn enforce_loopback_preconditions(
    peer: &SocketAddr,
    headers: &axum::http::HeaderMap,
) -> Result<(), StatusCode> {
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
    if !is_loopback_host(host) {
        tracing::warn!(host = %host, "/api/desktop/tokens rejected non-loopback Host");
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(())
}

/// Translate a `SecretsStore::set` outcome into the HTTP contract:
/// `StoreLocked` is the documented 503 surface (the Tauri side translates
/// it into an "unlock and retry" prompt). Every other failure is a 500.
fn classify_secret_write(result: Result<(), SecretsError>, key: &str) -> Result<(), StatusCode> {
    match result {
        Ok(()) => Ok(()),
        Err(SecretsError::StoreLocked) => {
            tracing::warn!(%key, "desktop token store rejected: daemon is locked");
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
        Err(e) => {
            tracing::error!(%key, error = %e, "secrets.set failed");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Translate a `SecretsStore::delete` outcome into the HTTP contract.
/// `NotFound` is treated as success because DELETE is idempotent: the
/// caller asked for the key gone, the key is gone. Locked store still
/// surfaces as 503. Other failures are 500.
fn classify_secret_delete(result: Result<(), SecretsError>, key: &str) -> Result<(), StatusCode> {
    match result {
        Ok(()) | Err(SecretsError::NotFound { .. }) => Ok(()),
        Err(SecretsError::StoreLocked) => {
            tracing::warn!(%key, "desktop token delete rejected: daemon is locked");
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
        Err(e) => {
            tracing::error!(%key, error = %e, "secrets.delete failed");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Return `true` when the raw `Host` header names a loopback target.
///
/// Handles three forms: bare hostname (`localhost`), IPv4 with optional
/// port (`127.0.0.1`, `127.0.0.1:19898`), and bracketed IPv6 with
/// optional port (`[::1]`, `[::1]:19898`). A naive `split(':')` breaks
/// the IPv6 case because every IPv6 address contains colons.
fn is_loopback_host(raw: &str) -> bool {
    if let Some(rest) = raw.strip_prefix('[') {
        // IPv6 bracketed literal. Take everything up to the matching `]`.
        let addr = rest.split(']').next().unwrap_or("");
        return addr == "::1";
    }
    let host_name = raw.split(':').next().unwrap_or("");
    matches!(host_name, "127.0.0.1" | "localhost")
}

fn rollback_access_token(secrets: &SecretsStore) {
    if let Err(e) = secrets.delete("entra_access_token") {
        tracing::error!(
            error = %e,
            "failed to roll back entra_access_token after refresh_token write failed; \
             operator cleanup required"
        );
    }
}
