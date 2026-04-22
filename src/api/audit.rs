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
    if accept_prefers_csv(accept) {
        let mut body = String::new();
        body.push_str("seq,timestamp,principal_key,action,resource_type,resource_id,result\n");
        for r in &rows {
            body.push_str(&format!(
                "{},{},{},{},{},{},{}\n",
                r.seq,
                csv_escape(&r.timestamp),
                csv_escape(&r.principal_key),
                csv_escape(&r.action),
                csv_escape(r.resource_type.as_deref().unwrap_or("")),
                csv_escape(r.resource_id.as_deref().unwrap_or("")),
                csv_escape(&r.result),
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

/// RFC 4180 CSV field escaping + Excel formula-injection guard.
/// PR #106 remediation C2: the admin audit CSV export is consumed by
/// compliance tooling and sometimes opened in spreadsheet apps; both
/// paths require escaping.
///
/// Quote and double-embedded-quote the field if it contains any of:
/// comma, double-quote, CR, or LF (RFC 4180 rule).
///
/// Prepend a single-quote sigil when the raw field starts with
/// `=`, `+`, `-`, `@`, CR, or LF (Excel formula-injection guard per
/// OWASP). The sigil is inside the quoted value so Excel sees it as
/// a leading text character and refuses to evaluate the cell. The
/// character is visible in the rendered CSV but not in tools that
/// post-process with a proper CSV parser.
fn csv_escape(field: &str) -> String {
    // Formula-injection guard first; then RFC 4180 quoting covers the
    // (possibly-prefixed) value uniformly.
    let needs_formula_guard = field
        .as_bytes()
        .first()
        .is_some_and(|b| matches!(*b, b'=' | b'+' | b'-' | b'@' | b'\r' | b'\n'));
    let needs_quotes = needs_formula_guard
        || field
            .as_bytes()
            .iter()
            .any(|b| matches!(*b, b',' | b'"' | b'\r' | b'\n'));
    if !needs_quotes {
        return field.to_string();
    }
    let escaped = field.replace('"', "\"\"");
    if needs_formula_guard {
        format!("\"'{escaped}\"")
    } else {
        format!("\"{escaped}\"")
    }
}

/// Accept-header parsing that correctly handles quality values and
/// multiple accepted types. PR #106 remediation I5: the prior
/// `.contains("text/csv")` would match `Accept: application/x-ndjson;q=1.0, text/csv;q=0.1`
/// and incorrectly downgrade to CSV.
///
/// Semantics kept intentionally narrow: if CSV appears ahead of NDJSON
/// in the header (or NDJSON is absent), prefer CSV. Otherwise NDJSON is
/// the default. Quality weights are ignored for simplicity — operator
/// tooling that wants CSV will send `Accept: text/csv` unambiguously.
fn accept_prefers_csv(accept: &str) -> bool {
    // Look at the first matching media type. Split on comma, trim, take
    // the part before any parameter (`;`).
    for raw in accept.split(',') {
        let media = raw.split(';').next().unwrap_or("").trim();
        if media.eq_ignore_ascii_case("text/csv") {
            return true;
        }
        if media.eq_ignore_ascii_case("application/x-ndjson")
            || media.eq_ignore_ascii_case("application/json")
        {
            return false;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{accept_prefers_csv, csv_escape};

    #[test]
    fn csv_escape_passes_through_plain_fields() {
        assert_eq!(csv_escape("alice"), "alice");
        assert_eq!(csv_escape("2026-04-22T12:00:00Z"), "2026-04-22T12:00:00Z");
    }

    #[test]
    fn csv_escape_quotes_and_doubles_embedded_commas_and_quotes() {
        assert_eq!(csv_escape("a,b"), "\"a,b\"");
        assert_eq!(csv_escape("a\"b"), "\"a\"\"b\"");
        assert_eq!(csv_escape("a\nb"), "\"a\nb\"");
    }

    #[test]
    fn csv_escape_guards_formula_injection() {
        // Leading =, +, -, @, CR, LF must be prefixed with a sigil and quoted.
        assert_eq!(csv_escape("=1+1"), "\"'=1+1\"");
        assert_eq!(csv_escape("+abc"), "\"'+abc\"");
        assert_eq!(csv_escape("-42"), "\"'-42\"");
        assert_eq!(csv_escape("@SUM(A1)"), "\"'@SUM(A1)\"");
    }

    #[test]
    fn accept_prefers_csv_first_token_wins() {
        assert!(accept_prefers_csv("text/csv"));
        assert!(accept_prefers_csv("text/csv;q=1.0"));
        assert!(accept_prefers_csv("text/csv, application/x-ndjson"));
        assert!(!accept_prefers_csv("application/x-ndjson, text/csv"));
        assert!(!accept_prefers_csv("application/x-ndjson;q=1.0, text/csv;q=0.1"));
        assert!(!accept_prefers_csv(""));
        assert!(!accept_prefers_csv("application/json"));
    }

    #[test]
    fn accept_prefers_csv_is_case_insensitive() {
        assert!(accept_prefers_csv("TEXT/CSV"));
        assert!(accept_prefers_csv("Text/Csv"));
    }
}
