//! Admin endpoint for claiming orphaned resources. Writes a
//! `resource_ownership` row for a `resource_type` / `resource_id`
//! pair with no existing owner, gated by the `SpacebotAdmin` role
//! and emits an audit event on success.

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
#[serde(deny_unknown_fields)]
pub(super) struct ClaimRequest {
    pub resource_type: String,
    pub resource_id: String,
    pub owner_principal_key: String,
    #[serde(default)]
    pub visibility: Visibility,
    #[serde(default)]
    pub shared_with_team_id: Option<String>,
}

#[utoipa::path(
    post,
    path = "/admin/claim-resource",
    request_body = ClaimRequest,
    responses(
        (status = 200, description = "Ownership row written"),
        (status = 403, description = "Caller is not a SpacebotAdmin"),
        (status = 422, description = "Request body failed validation"),
        (status = 500, description = "Pool unavailable or write failed"),
    ),
    tag = "admin",
)]
pub(super) async fn claim_resource(
    State(state): State<Arc<ApiState>>,
    axum::Extension(ctx): axum::Extension<AuthContext>,
    Json(req): Json<ClaimRequest>,
) -> Result<StatusCode, StatusCode> {
    if let Err(error) = require_role(&ctx, ROLE_ADMIN) {
        tracing::warn!(
            principal_key = %ctx.principal_key(),
            required_role = ROLE_ADMIN,
            %error,
            "admin_claim denied: missing role",
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
    if let Err(error) = set_ownership(
        &pool,
        &req.resource_type,
        &req.resource_id,
        None,
        &req.owner_principal_key,
        req.visibility,
        req.shared_with_team_id.as_deref(),
    )
    .await
    {
        tracing::error!(
            principal_key = %ctx.principal_key(),
            resource_type = %req.resource_type,
            resource_id = %req.resource_id,
            %error,
            "admin_claim: set_ownership failed",
        );
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    if let Some(audit) = state.audit.load().as_ref().as_ref().cloned() {
        let actor = ctx.principal_key();
        let rt = req.resource_type.clone();
        let rid = req.resource_id.clone();
        let owner = req.owner_principal_key.clone();
        tokio::spawn(async move {
            if let Err(error) = audit
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
                .await
            {
                tracing::warn!(%error, "audit append failed: admin_claim_resource event dropped");
            }
        });
    }

    Ok(StatusCode::OK)
}
