//! Entra-JWT auth middleware. Companion to the static-token middleware in
//! `src/api/server.rs::api_auth_middleware`. The two are **branches**, not
//! composed layers (research §11.2(5), §12 A-Alternative-3).
//!
//! Selection happens at middleware install time in `start_http_server`:
//! `ApiState.entra_auth` populated => install this. Otherwise install the
//! static-token middleware. They are never both attached.

use axum::Json;
use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use std::sync::Arc;

use crate::api::ApiState;
use crate::auth::{AuthError, EntraValidator};

pub async fn entra_auth_middleware(
    State(state): State<Arc<ApiState>>,
    mut request: Request,
    next: Next,
) -> Response {
    // Health bypass, matching the static-token middleware.
    let path = request.uri().path();
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

    let bearer_opt = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string);

    let result = match bearer_opt {
        Some(token) => validator.validate(&token).await,
        None => Err(AuthError::MissingHeader),
    };

    match result {
        Ok(ctx) => {
            request.extensions_mut().insert(ctx);
            next.run(request).await
        }
        Err(err) => {
            #[cfg(feature = "metrics")]
            crate::telemetry::Metrics::global()
                .auth_failures_total
                .with_label_values(&["entra_jwt", err.metric_reason()])
                .inc();
            // Log at debug for expected-bad inputs, warn for server-side issues.
            match &err {
                AuthError::JwksUnreachable => {
                    tracing::warn!(?err, "entra auth rejected: infra issue");
                }
                _ => {
                    tracing::debug!(?err, "entra auth rejected");
                }
            }
            (err.status(), Json(json!({"error": err.to_string()}))).into_response()
        }
    }
}
