//! Admin endpoint for claiming orphaned resources (Phase 2 backfill
//! policy). Writes a `resource_ownership` row for a `resource_type` /
//! `resource_id` pair with no existing owner, gated by the
//! `SpacebotAdmin` role and emits a Phase-5 audit event on success.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;

use crate::api::state::ApiState;
use crate::auth::context::AuthContext;
use crate::auth::principals::Visibility;
use crate::auth::repository::set_ownership;
use crate::auth::roles::{ROLE_ADMIN, require_role};

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct ClaimRequest {
    pub resource_type: String,
    pub resource_id: String,
    pub owner_principal_key: String,
    #[serde(default = "default_vis")]
    pub visibility: String,
    #[serde(default)]
    pub shared_with_team_id: Option<String>,
}

fn default_vis() -> String {
    "personal".into()
}

#[utoipa::path(
    post,
    path = "/admin/claim-resource",
    request_body = ClaimRequest,
    responses(
        (status = 200, description = "Ownership row written"),
        (status = 400, description = "Invalid visibility value"),
        (status = 403, description = "Caller is not a SpacebotAdmin"),
        (status = 500, description = "Pool unavailable or write failed"),
    ),
    tag = "admin",
)]
pub(super) async fn claim_resource(
    State(state): State<Arc<ApiState>>,
    axum::Extension(ctx): axum::Extension<AuthContext>,
    Json(req): Json<ClaimRequest>,
) -> Result<StatusCode, StatusCode> {
    require_role(&ctx, ROLE_ADMIN).map_err(|_| StatusCode::FORBIDDEN)?;
    let pool = state
        .instance_pool
        .load()
        .as_ref()
        .as_ref()
        .cloned()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let vis = Visibility::parse(&req.visibility).ok_or(StatusCode::BAD_REQUEST)?;
    set_ownership(
        &pool,
        &req.resource_type,
        &req.resource_id,
        None,
        &req.owner_principal_key,
        vis,
        req.shared_with_team_id.as_deref(),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(audit) = state.audit.load().as_ref().as_ref().cloned() {
        let actor = ctx.principal_key();
        let rt = req.resource_type.clone();
        let rid = req.resource_id.clone();
        let owner = req.owner_principal_key.clone();
        tokio::spawn(async move {
            let _ = audit
                .append(crate::audit::AuditEvent {
                    principal_key: actor,
                    principal_type: "user".into(),
                    action: crate::audit::AuditAction::AdminClaimResource,
                    resource_type: Some(rt),
                    resource_id: Some(rid),
                    result: "allowed".into(),
                    source_ip: None,
                    request_id: None,
                    metadata: serde_json::json!({ "claimed_for": owner }),
                })
                .await;
        });
    }

    Ok(StatusCode::OK)
}
