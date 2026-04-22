//! Admin-only audit read + chain-verify endpoints. Returns NDJSON by
//! default; CSV available via `Accept: text/csv` header. The `/verify`
//! handler runs [`crate::audit::AuditAppender::verify_chain`] and
//! returns its [`ChainVerifyResult`] as JSON.

use super::state::ApiState;

use crate::audit::types::AuditRow;
use crate::auth::context::AuthContext;
use crate::auth::roles::{ROLE_ADMIN, require_role};

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use std::sync::Arc;

/// Query-string filters for `GET /api/admin/audit`. All fields optional;
/// missing fields are skipped via `(? IS NULL OR col = ?)` in the SQL.
#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub(super) struct AuditQuery {
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub principal: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    100
}

/// Response body of `GET /api/admin/audit/verify`. Matches the fields of
/// [`crate::audit::appender::ChainVerifyResult`]; when the chain is
/// intact `first_mismatch_seq` is `None`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(super) struct VerifyChainResponse {
    pub valid: bool,
    pub first_mismatch_seq: Option<i64>,
    pub total_rows: i64,
}

/// Admin-only paginated audit read. NDJSON by default; `Accept: text/csv`
/// returns CSV with a fixed column order.
#[utoipa::path(
    get,
    path = "/admin/audit",
    params(AuditQuery),
    responses(
        (status = 200, description = "NDJSON (or CSV) stream of audit rows"),
        (status = 403, description = "Caller lacks SpacebotAdmin role"),
        (status = 500, description = "Instance pool unavailable or query failed"),
    ),
    tag = "audit",
)]
pub(super) async fn list_audit_events(
    State(state): State<Arc<ApiState>>,
    auth_ctx: AuthContext,
    Query(q): Query<AuditQuery>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    require_role(&auth_ctx, ROLE_ADMIN).map_err(|_| StatusCode::FORBIDDEN)?;
    let pool = state
        .instance_pool
        .load()
        .as_ref()
        .as_ref()
        .cloned()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Dynamic WHERE with fixed-position binds. Each filter is emitted
    // twice in the query text (first as the NULL guard, then as the
    // equality comparison) so we bind each value twice.
    let limit = q.limit.clamp(1, 1000);
    let offset = q.offset.max(0);
    let rows: Vec<AuditRow> = sqlx::query_as(
        r#"
        SELECT * FROM audit_events
        WHERE (? IS NULL OR timestamp >= ?)
          AND (? IS NULL OR timestamp <= ?)
          AND (? IS NULL OR principal_key = ?)
          AND (? IS NULL OR action = ?)
        ORDER BY seq DESC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(&q.from)
    .bind(&q.from)
    .bind(&q.to)
    .bind(&q.to)
    .bind(&q.principal)
    .bind(&q.principal)
    .bind(&q.action)
    .bind(&q.action)
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        tracing::error!(?e, "audit query failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let accept = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if accept.contains("text/csv") {
        let mut body = String::new();
        body.push_str("seq,timestamp,principal_key,action,resource_type,resource_id,result\n");
        for r in &rows {
            body.push_str(&format!(
                "{},{},{},{},{},{},{}\n",
                r.seq,
                r.timestamp,
                r.principal_key,
                r.action,
                r.resource_type.as_deref().unwrap_or(""),
                r.resource_id.as_deref().unwrap_or(""),
                r.result,
            ));
        }
        Ok(([(header::CONTENT_TYPE, "text/csv")], body).into_response())
    } else {
        let mut body = String::new();
        for r in &rows {
            body.push_str(
                &serde_json::to_string(&serde_json::json!({
                    "seq": r.seq,
                    "id": r.id,
                    "timestamp": r.timestamp,
                    "principal_key": r.principal_key,
                    "principal_type": r.principal_type,
                    "action": r.action,
                    "resource_type": r.resource_type,
                    "resource_id": r.resource_id,
                    "result": r.result,
                    "source_ip": r.source_ip,
                    "request_id": r.request_id,
                    "metadata": serde_json::from_str::<serde_json::Value>(&r.metadata_json)
                        .unwrap_or(serde_json::Value::Null),
                    "prev_hash": r.prev_hash,
                    "row_hash": r.row_hash,
                }))
                .unwrap_or_else(|_| "{}".into()),
            );
            body.push('\n');
        }
        Ok(([(header::CONTENT_TYPE, "application/x-ndjson")], body).into_response())
    }
}

/// Admin-only chain integrity probe. Returns `503` if the instance-level
/// appender is not yet attached (startup window before
/// `set_instance_pool`).
#[utoipa::path(
    get,
    path = "/admin/audit/verify",
    responses(
        (status = 200, description = "Chain integrity result", body = VerifyChainResponse),
        (status = 403, description = "Caller lacks SpacebotAdmin role"),
        (status = 500, description = "Verify query failed"),
        (status = 503, description = "Audit appender not yet attached"),
    ),
    tag = "audit",
)]
pub(super) async fn verify_audit_chain(
    State(state): State<Arc<ApiState>>,
    auth_ctx: AuthContext,
) -> Result<Json<VerifyChainResponse>, StatusCode> {
    require_role(&auth_ctx, ROLE_ADMIN).map_err(|_| StatusCode::FORBIDDEN)?;
    let appender = state
        .audit
        .load()
        .as_ref()
        .as_ref()
        .cloned()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let result = appender.verify_chain().await.map_err(|e| {
        tracing::error!(?e, "audit chain verify failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(VerifyChainResponse {
        valid: result.valid,
        first_mismatch_seq: result.first_mismatch_seq,
        total_rows: result.total_rows,
    }))
}
