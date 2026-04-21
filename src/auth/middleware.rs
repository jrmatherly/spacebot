//! Entra-JWT auth middleware. Companion to the static-token middleware in
//! `src/api/server.rs::api_auth_middleware`. The two are branches, not
//! composed layers: `start_http_server` chooses one at install time based on
//! whether `ApiState.entra_auth` is populated.

use crate::api::ApiState;
use crate::auth::{AuthError, EntraValidator};

use axum::Json;
use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use tracing::Instrument as _;

use std::sync::Arc;

pub async fn entra_auth_middleware(
    State(state): State<Arc<ApiState>>,
    mut request: Request,
    next: Next,
) -> Response {
    // Health bypass, matching the static-token middleware.
    let path = request.uri().path().to_string();
    if path == "/api/health" || path == "/health" {
        return next.run(request).await;
    }

    let guard = state.entra_auth.load();
    let validator: &EntraValidator = match guard.as_ref() {
        Some(v) => v.as_ref(),
        None => {
            tracing::error!("entra_auth_middleware attached but validator absent");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "auth misconfigured"})),
            )
                .into_response();
        }
    };

    // Parse the Authorization header explicitly so we can distinguish
    // "absent" from "present but malformed" (non-UTF8, wrong scheme). The
    // static-token middleware makes the same distinction via
    // `AuthRejectReason::HeaderNonAscii` / `SchemeMissing`.
    let bearer_result: Result<String, AuthError> =
        match request.headers().get(header::AUTHORIZATION) {
            None => Err(AuthError::MissingHeader),
            Some(v) => match v.to_str() {
                Err(_) => Err(AuthError::MalformedHeader),
                Ok(raw) => match raw.strip_prefix("Bearer ") {
                    None => Err(AuthError::MalformedHeader),
                    Some(token) => Ok(token.to_string()),
                },
            },
        };

    // Clone the bearer before `validator.validate` consumes it. The OBO
    // flow in Phase 3's group-sync spawn needs the original user token
    // (assertion in the OAuth2 OBO grant), not the parsed AuthContext.
    let bearer_token: Option<String> = bearer_result.as_ref().ok().cloned();

    let result = match bearer_result {
        Ok(token) => validator.validate(&token).await,
        Err(err) => Err(err),
    };

    match result {
        Ok(ctx) => {
            // Fire-and-forget user upsert. The request itself proceeds
            // regardless. Upsert failures are logged and counted for
            // operational audit (SOC 2 completeness of principal records).
            if let Some(pool) = state.instance_pool.load().as_ref().clone() {
                let ctx_for_task = ctx.clone();
                let principal_key = ctx.principal_key();
                let upsert_span = tracing::info_span!(
                    "auth.upsert_user",
                    principal_key = %principal_key,
                );
                tokio::spawn(
                    async move {
                        if let Err(e) =
                            crate::auth::repository::upsert_user_from_auth(&pool, &ctx_for_task)
                                .await
                        {
                            let reason = match e {
                                crate::auth::repository::RepositoryError::InvalidPrincipalType => {
                                    "invalid_principal_type"
                                }
                                crate::auth::repository::RepositoryError::Sqlx(_) => "sqlx",
                            };
                            #[cfg(feature = "metrics")]
                            crate::telemetry::Metrics::global()
                                .auth_upsert_failures_total
                                .with_label_values(&[reason])
                                .inc();
                            tracing::error!(
                                reason,
                                error = %e,
                                "upsert_user_from_auth failed",
                            );
                        }
                    }
                    .instrument(upsert_span),
                );
            } else {
                // Should only happen during early startup. A persistent
                // non-zero counter value means `set_instance_pool` ran
                // after the HTTP server was accepting requests.
                #[cfg(feature = "metrics")]
                crate::telemetry::Metrics::global()
                    .auth_upsert_skipped_total
                    .inc();
                tracing::error!(
                    principal_key = %ctx.principal_key(),
                    "auth succeeded but instance_pool is not attached; \
                     user row upsert skipped",
                );
            }

            // Phase 3: resolve Graph group memberships when overage is set
            // OR sync claim-provided groups, AND fetch the user's display
            // photo (A-19). Same OBO token (User.Read) covers both. Skipped
            // silently when Graph is unwired (deployments without
            // ENTRA_GRAPH_CLIENT_SECRET).
            //
            // A-11: pagination lives inside `list_member_groups` for the
            // /groups?$filter lookup. `getMemberObjects` itself does not
            // paginate (OData action).
            let graph_guard = state.graph_client.load();
            if let Some(graph) = graph_guard.as_ref().as_ref().map(Arc::clone) {
                let pool_opt = state.instance_pool.load().as_ref().clone();
                if let (Some(pool), Some(user_token)) = (pool_opt, bearer_token.clone()) {
                    let ttl_secs = state
                        .entra_auth
                        .load()
                        .as_ref()
                        .as_ref()
                        .map(|v| v.config().group_cache_ttl_secs)
                        .unwrap_or(300);

                    // Group sync (overage resolution + team_memberships persist).
                    let group_pool = pool.clone();
                    let group_graph = Arc::clone(&graph);
                    let group_token = user_token.clone();
                    let group_ctx = ctx.clone();
                    let group_span = tracing::info_span!(
                        "auth.sync_groups",
                        principal_key = %ctx.principal_key(),
                    );
                    tokio::spawn(
                        async move {
                            if let Err(error) = sync_groups_for_principal(
                                &group_pool,
                                &group_graph,
                                &group_ctx,
                                &group_token,
                                ttl_secs,
                            )
                            .await
                            {
                                tracing::warn!(
                                    %error,
                                    "group sync failed; team/org authz fail-closed",
                                );
                            }
                        }
                        .instrument(group_span),
                    );

                    // A-19 photo sync. Same OBO scope. Weekly TTL inside helper.
                    let photo_ctx = ctx.clone();
                    let photo_span = tracing::info_span!(
                        "auth.sync_photo",
                        principal_key = %ctx.principal_key(),
                    );
                    tokio::spawn(
                        async move {
                            if let Err(error) = sync_user_photo_for_principal(
                                &pool,
                                &graph,
                                &photo_ctx,
                                &user_token,
                            )
                            .await
                            {
                                tracing::warn!(
                                    %error,
                                    "photo sync failed; SPA falls back to initials",
                                );
                            }
                        }
                        .instrument(photo_span),
                    );
                }
            }

            // A-10: first-request race. When a user has just authenticated
            // and the async group sync hasn't persisted memberships yet,
            // return 202 + Retry-After: 2 so the SPA retries instead of
            // surfacing spurious 404s on team-scoped resources.
            //
            // Only fires when the token signals the user SHOULD have
            // memberships (groups_overage OR non-empty groups claim). A
            // user with a legitimately empty membership set proceeds
            // normally and never sees 202.
            if ctx.principal_type == crate::auth::PrincipalType::User
                && let Some(pool) = state.instance_pool.load().as_ref().clone()
                && (ctx.groups_overage || !ctx.groups.is_empty())
            {
                let key = ctx.principal_key();
                let has_memberships: Option<i64> = sqlx::query_scalar(
                    "SELECT 1 FROM team_memberships WHERE principal_key = ? LIMIT 1",
                )
                .bind(&key)
                .fetch_optional(&pool)
                .await
                .unwrap_or(None);
                if has_memberships.is_none() {
                    #[cfg(feature = "metrics")]
                    crate::telemetry::Metrics::global()
                        .auth_first_request_race_total
                        .inc();
                    tracing::debug!(
                        principal_key = %key,
                        "first-request race: returning 202 Accepted",
                    );
                    return (
                        StatusCode::ACCEPTED,
                        [(header::RETRY_AFTER, "2")],
                        Json(json!({
                            "status": "syncing_permissions",
                            "retry_after_seconds": 2,
                        })),
                    )
                        .into_response();
                }
            }

            request.extensions_mut().insert(ctx);
            next.run(request).await
        }
        Err(err) => {
            let reason = err.metric_reason();
            #[cfg(feature = "metrics")]
            crate::telemetry::Metrics::global()
                .auth_failures_total
                .with_label_values(&["entra_jwt", reason])
                .inc();
            // Match the static-token branch's visibility: auth rejections
            // land at `warn!` so default `RUST_LOG=info` deployments see
            // brute-force probing without requiring a dashboard.
            tracing::warn!(reason, %path, "entra auth rejected");
            (err.status(), Json(json!({"error": err.to_string()}))).into_response()
        }
    }
}

/// Resolve group memberships for an authenticated principal and persist
/// them into `team_memberships`. Called from `entra_auth_middleware` as a
/// fire-and-forget spawn after successful auth.
///
/// Behaviour:
/// - When `ctx.groups_overage` is true, calls Graph `/me/getMemberObjects`
///   to enumerate transitive memberships.
/// - Otherwise, uses the `groups` claim already on the JWT (no Graph call).
/// - Replaces all rows for `principal_key` (delete-and-insert) so revoked
///   memberships propagate.
///
/// `#[doc(hidden)] pub` so integration tests in `tests/*.rs` (separate
/// crates without `cfg(test)` visibility) can drive it directly.
/// `ttl_secs` short-circuits the Graph call when persisted memberships
/// are younger than the configured TTL (default 300s).
#[doc(hidden)]
pub async fn sync_groups_for_principal(
    pool: &sqlx::SqlitePool,
    graph: &crate::auth::graph::GraphClient,
    ctx: &crate::auth::AuthContext,
    user_token: &str,
    ttl_secs: u64,
) -> anyhow::Result<()> {
    use crate::auth::repository::upsert_team;

    let principal_key = ctx.principal_key();

    // Cache TTL skip: if any existing membership row is younger than the
    // configured TTL, treat the cached set as authoritative and don't
    // hammer Graph on every request. MIN(observed_at) is the oldest of
    // the persisted rows; if it is fresh, all of them are.
    let oldest: Option<String> =
        sqlx::query_scalar("SELECT MIN(observed_at) FROM team_memberships WHERE principal_key = ?")
            .bind(&principal_key)
            .fetch_optional(pool)
            .await?
            .flatten();

    if let Some(ts) = oldest
        && let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&ts)
    {
        let age = chrono::Utc::now().signed_duration_since(dt.with_timezone(&chrono::Utc));
        if age < chrono::Duration::seconds(ttl_secs as i64) {
            return Ok(());
        }
    }

    let groups = if ctx.groups_overage || ctx.groups.is_empty() {
        graph.list_member_groups(user_token).await?
    } else {
        ctx.groups
            .iter()
            .map(|g| crate::auth::graph::GraphGroup {
                id: g.to_string(),
                display_name: None,
            })
            .collect()
    };

    // Upsert teams OUTSIDE the transaction. `upsert_team` is individually
    // idempotent (INSERT ... ON CONFLICT DO UPDATE), so a partial failure
    // across teams just leaves extra `teams` rows with correct data — not
    // an authz concern. Teams without memberships are inert.
    let source = if ctx.groups_overage {
        "graph_overage"
    } else {
        "token_claim"
    };
    let mut team_ids: Vec<String> = Vec::with_capacity(groups.len());
    for group in groups {
        let display = group.display_name.as_deref().unwrap_or("(unnamed)");
        let team = upsert_team(pool, &group.id, display).await?;
        team_ids.push(team.id);
    }

    // Atomic replace-set on `team_memberships`. Without the transaction,
    // a crash or sqlx error between DELETE and the last INSERT leaves the
    // principal with a PARTIAL set of memberships, causing Phase 4 to
    // silently 403 resources the user actually has access to. Commit only
    // succeeds when the full new set is persisted.
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM team_memberships WHERE principal_key = ?")
        .bind(&principal_key)
        .execute(&mut *tx)
        .await?;
    for team_id in &team_ids {
        sqlx::query(
            r#"
            INSERT INTO team_memberships (principal_key, team_id, source)
            VALUES (?, ?, ?)
            ON CONFLICT(principal_key, team_id) DO UPDATE SET
                observed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                source = excluded.source
            "#,
        )
        .bind(&principal_key)
        .bind(team_id)
        .bind(source)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Fetch the signed-in user's photo via Graph and persist into
/// `users.display_photo_b64` (A-19). Weekly TTL via `photo_updated_at`.
/// Skips the Graph call when the existing timestamp is younger than 7
/// days. A confirmed-absent photo (404) writes NULL bytes but stamps
/// `now`, so the next refresh is also one week out.
///
/// `#[doc(hidden)] pub` for the same reason as `sync_groups_for_principal`:
/// Phase 3 integration tests live in a separate crate.
#[doc(hidden)]
pub async fn sync_user_photo_for_principal(
    pool: &sqlx::SqlitePool,
    graph: &crate::auth::graph::GraphClient,
    ctx: &crate::auth::AuthContext,
    user_token: &str,
) -> anyhow::Result<()> {
    use base64::Engine as _;

    let principal_key = ctx.principal_key();

    let last: Option<String> = sqlx::query_scalar::<_, Option<String>>(
        "SELECT photo_updated_at FROM users WHERE principal_key = ?",
    )
    .bind(&principal_key)
    .fetch_optional(pool)
    .await?
    .flatten();

    if let Some(ts) = last
        && let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&ts)
    {
        let age = chrono::Utc::now().signed_duration_since(dt.with_timezone(&chrono::Utc));
        if age < chrono::Duration::days(7) {
            return Ok(());
        }
    }

    let bytes_opt = graph.fetch_user_photo(user_token).await?;
    let b64_opt = bytes_opt
        .as_deref()
        .map(|b| base64::engine::general_purpose::STANDARD.encode(b));

    crate::auth::repository::upsert_user_photo(pool, &principal_key, b64_opt.as_deref()).await?;
    Ok(())
}
