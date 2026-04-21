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

    let result = match bearer_result {
        Ok(token) => validator.validate(&token).await,
        Err(err) => Err(err),
    };

    match result {
        Ok(ctx) => {
            // Fire-and-forget user upsert. The request itself proceeds
            // regardless; upsert failures are logged for operational audit.
            if let Some(pool) = state.instance_pool.load().as_ref().clone() {
                let ctx_for_task = ctx.clone();
                tokio::spawn(async move {
                    if let Err(e) =
                        crate::auth::repository::upsert_user_from_auth(&pool, &ctx_for_task).await
                    {
                        tracing::warn!(?e, "upsert_user_from_auth failed");
                    }
                });
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
