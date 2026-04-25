//! Admin endpoint for the SOC 2 quarterly access-review evidence (CC6.7).
//! Emits a per-principal report covering identity, status, last-seen
//! timestamp, and team memberships in either CSV (default) or JSON form.
//! Admin-gated; an `AdminRead` audit event records each invocation.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::api::state::ApiState;
use crate::auth::context::AuthContext;
use crate::auth::roles::{ROLE_ADMIN, require_role};

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ReviewQuery {
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_format() -> String {
    "csv".into()
}

#[derive(sqlx::FromRow)]
struct Row {
    principal_key: String,
    display_name: Option<String>,
    display_email: Option<String>,
    principal_type: String,
    status: String,
    last_seen_at: Option<String>,
    team_names: Option<String>,
}

#[utoipa::path(
    get,
    path = "/admin/access-review",
    params(("format" = Option<String>, Query, description = "csv (default) or json")),
    responses(
        (status = 200, description = "Access-review report (CSV or JSON)"),
        (status = 403, description = "Caller is not a SpacebotAdmin"),
        (status = 500, description = "Pool unavailable or query failed"),
    ),
    tag = "admin",
)]
pub(super) async fn access_review(
    State(state): State<Arc<ApiState>>,
    axum::Extension(ctx): axum::Extension<AuthContext>,
    Query(q): Query<ReviewQuery>,
) -> Result<Response, StatusCode> {
    if let Err(error) = require_role(&ctx, ROLE_ADMIN) {
        tracing::warn!(
            principal_key = %ctx.principal_key(),
            required_role = ROLE_ADMIN,
            %error,
            "access_review denied: missing role",
        );
        return Err(StatusCode::FORBIDDEN);
    }
    let pool = state
        .instance_pool
        .load()
        .as_ref()
        .as_ref()
        .cloned()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows: Vec<Row> = sqlx::query_as(
        r#"
        SELECT
            u.principal_key,
            u.display_name,
            u.display_email,
            u.principal_type,
            u.status,
            u.last_seen_at,
            GROUP_CONCAT(t.display_name, '; ') AS team_names
        FROM users u
        LEFT JOIN team_memberships tm ON tm.principal_key = u.principal_key
        LEFT JOIN teams t ON t.id = tm.team_id
        GROUP BY u.principal_key
        ORDER BY u.display_name, u.principal_key
        "#,
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, "access_review: query failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if let Some(audit) = state.audit.load().as_ref().as_ref().cloned() {
        let actor = ctx.principal_key();
        let principal_type = ctx.principal_type.as_canonical_str().to_string();
        let row_count = rows.len();
        tokio::spawn(async move {
            if let Err(error) = audit
                .append(crate::audit::AuditEvent {
                    principal_key: actor,
                    principal_type,
                    action: crate::audit::AuditAction::AdminRead,
                    resource_type: Some("access_review".into()),
                    resource_id: None,
                    result: "allowed".into(),
                    source_ip: None,
                    request_id: None,
                    metadata: serde_json::json!({ "row_count": row_count }),
                })
                .await
            {
                tracing::warn!(%error, "audit append failed: access_review event dropped");
            }
        });
    }

    let response = if q.format == "json" {
        let data: Vec<serde_json::Value> = rows
            .iter()
            .map(|r| {
                let teams: Vec<&str> = r
                    .team_names
                    .as_deref()
                    .map(|s| s.split("; ").filter(|t| !t.is_empty()).collect())
                    .unwrap_or_default();
                serde_json::json!({
                    "principal_key": r.principal_key,
                    "display_name": r.display_name,
                    "display_email": r.display_email,
                    "principal_type": r.principal_type,
                    "status": r.status,
                    "last_seen_at": r.last_seen_at,
                    "teams": teams,
                })
            })
            .collect();
        axum::Json(data).into_response()
    } else {
        let mut body = String::new();
        body.push_str(
            "principal_key,display_name,display_email,principal_type,status,last_seen_at,teams\n",
        );
        for r in &rows {
            body.push_str(&format!(
                "{},{},{},{},{},{},{}\n",
                escape_csv(&r.principal_key),
                escape_csv(r.display_name.as_deref().unwrap_or("")),
                escape_csv(r.display_email.as_deref().unwrap_or("")),
                escape_csv(&r.principal_type),
                escape_csv(&r.status),
                escape_csv(r.last_seen_at.as_deref().unwrap_or("")),
                escape_csv(r.team_names.as_deref().unwrap_or("")),
            ));
        }
        ([(header::CONTENT_TYPE, "text/csv")], body).into_response()
    };
    Ok(response)
}

/// Escape a field per RFC 4180: wrap in quotes when the field contains a
/// comma, double-quote, or newline; double up any embedded quotes.
fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
