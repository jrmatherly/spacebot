//! Admin endpoint for the SOC 2 orphan-resource sweep evidence. Returns
//! the current list of MissingOwnership + StaleOwnership findings as
//! JSON. Admin-gated; the sweep emits an `AdminRead` audit event so the
//! cross-agent scan is itself logged.
//!
//! Manual on-demand evidence collection. Automated weekly scheduling
//! (with dry-run defaults and per-agent-DB AdminRead events) is a
//! follow-up that requires a new instance-level maintenance subsystem
//! outside the per-agent cortex.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Serialize;

use crate::admin::{Orphan, OrphanKind, discover_agent_db_paths, sweep_orphans};
use crate::api::state::ApiState;
use crate::auth::context::AuthContext;
use crate::auth::roles::{ROLE_ADMIN, require_role};

/// Wire-shape DTO for a single orphan finding. `kind` is the typed
/// [`OrphanKind`] enum (snake_case on the wire); the schema generator
/// emits a discriminated string union for TypeScript clients.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(super) struct OrphanReport {
    pub kind: OrphanKind,
    pub resource_type: String,
    pub resource_id: String,
    pub owning_agent_id: Option<String>,
}

/// Response body for `GET /admin/orphans`. `agent_dbs_scanned` is `u32`
/// for a deterministic wire width; the count is bounded well below 4
/// billion in practice.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(super) struct OrphansResponse {
    pub orphans: Vec<OrphanReport>,
    pub agent_dbs_scanned: u32,
}

impl From<Orphan> for OrphanReport {
    fn from(o: Orphan) -> Self {
        Self {
            kind: o.kind,
            resource_type: o.resource_type,
            resource_id: o.resource_id,
            owning_agent_id: o.owning_agent_id,
        }
    }
}

#[utoipa::path(
    get,
    path = "/admin/orphans",
    responses(
        (status = 200, description = "Orphan-resource sweep report", body = OrphansResponse),
        (status = 403, description = "Caller is not a SpacebotAdmin"),
        (status = 500, description = "Pool unavailable or sweep failed"),
    ),
    tag = "admin",
)]
pub(super) async fn list_orphans(
    State(state): State<Arc<ApiState>>,
    axum::Extension(ctx): axum::Extension<AuthContext>,
) -> Result<Json<OrphansResponse>, StatusCode> {
    if let Err(error) = require_role(&ctx, ROLE_ADMIN) {
        tracing::warn!(
            principal_key = %ctx.principal_key(),
            required_role = ROLE_ADMIN,
            %error,
            "admin_orphans denied: missing role",
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

    let agent_dbs = discover_agent_db_paths();
    // u32 on the wire keeps the schema deterministic across 32/64-bit
    // hosts; agent counts on a single instance are bounded well below
    // 4 billion so the saturating cast is a no-op in practice.
    let agent_dbs_scanned = u32::try_from(agent_dbs.len()).unwrap_or(u32::MAX);
    let orphans = sweep_orphans(&pool, &agent_dbs).await.map_err(|error| {
        tracing::error!(%error, "admin_orphans: sweep_orphans failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if let Some(audit) = state.audit.load().as_ref().as_ref().cloned() {
        let actor = ctx.principal_key();
        let principal_type = ctx.principal_type.as_canonical_str().to_string();
        let orphan_count = orphans.len();
        tokio::spawn(async move {
            if let Err(error) = audit
                .append(crate::audit::AuditEvent {
                    principal_key: actor,
                    principal_type,
                    action: crate::audit::AuditAction::AdminRead,
                    resource_type: Some("orphans".into()),
                    resource_id: None,
                    result: "allowed".into(),
                    source_ip: None,
                    request_id: None,
                    metadata: serde_json::json!({
                        "agent_dbs_scanned": agent_dbs_scanned,
                        "orphan_count": orphan_count,
                    }),
                })
                .await
            {
                tracing::warn!(%error, "audit append failed: admin_orphans event dropped");
            }
        });
    }

    Ok(Json(OrphansResponse {
        agent_dbs_scanned,
        orphans: orphans.into_iter().map(OrphanReport::from).collect(),
    }))
}
