//! Portal HTTP handlers + shared Phase-4 authz gate.
//!
//! All seven endpoints (`portal_send`, `portal_history`, `list_portal_conversations`,
//! `create_portal_conversation`, `update_portal_conversation`,
//! `delete_portal_conversation`, `conversation_defaults`) consult the
//! Phase-4 authz helpers with `resource_type = "portal_conversation"`
//! before touching the store. Access keys on the session_id (A-09: the
//! bare UUID-shaped session_id, no sigil'd prefix).
//!
//! **Personal-visibility invariant (non-negotiable):** every
//! `set_ownership` call in this file passes `Visibility::Personal`. Portal
//! conversations are private user chats. Per
//! `.scratchpad/plans/entraid-auth/phase-4-authz-helpers.md` §12 A-2,
//! this is the single most identity-sensitive table in the system; a
//! convenience default to `Visibility::Org` would leak one user's chat
//! history to the rest of the tenant. Never relax this without updating
//! the plan doc first. The regression test
//! `create_portal_conversation_assigns_personal_ownership` in
//! `tests/api_portal_authz.rs` guards this at runtime.
//!
//! `portal_send` has an auto-create path: if the `session_id` has no
//! ownership row yet, the handler treats the request as a create and
//! `.await`s `set_ownership` with `Visibility::Personal` before injecting
//! the message. If the session already exists, the handler calls
//! `check_write` instead. The branch is decided by a single
//! `get_ownership` call made up front; no TOCTOU window of consequence
//! exists because the subsequent `store.ensure` is idempotent and the
//! `set_ownership` upsert collapses races into a single row.
//!
//! `list_portal_conversations` gates on the caller's `agent_id` filter
//! via `"agent"` read-access (mirrors `list_tasks` and `list_memories`:
//! the agent filter identifies a single resource). A listing query that
//! omits `agent_id` cannot currently reach this handler (the type
//! requires it), so there is no Phase-5 unfiltered TODO.
//!
//! `conversation_defaults` is a catalog/config endpoint that returns
//! agent-scoped default settings and the configured model list. It
//! exposes no per-user conversation data, so it is intentionally
//! ungated: gating would require a caller-policy contract the endpoint
//! doesn't need. An `agent_id`-scoped read gate is a reasonable Phase-5
//! tightening (TODO below) once the audit log can absorb the N+1 cost.
//!
//! The ~45-line inline gate block mirrors `src/api/memories.rs` per
//! Phase 4 PR 2 decision N1: single-file grep-visibility beats DRY.
//! Pool-None is always-on `tracing::error!` + feature-gated
//! `spacebot_authz_skipped_total{handler="portal"}`. Metric label is
//! the file resource family (not `"portal_conversation"` singular), so
//! counter cardinality stays flat.

use super::state::ApiState;
use crate::{
    Attachment, InboundMessage, MessageContent,
    conversation::{
        ConversationDefaultsResponse, ConversationSettings, DelegationMode, MemoryMode,
        ModelOption, PortalConversation, PortalConversationStore, PortalConversationSummary,
        WorkerContextMode,
    },
};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Default visibility for portal_conversation ownership rows. Per
/// Phase-4 plan §12 A-2: portal chats are private user data; defaulting
/// to `Org` would leak conversations tenant-wide. The two `set_ownership`
/// call sites (`portal_send` auto-create branch and `create_portal_conversation`)
/// both read this constant so the invariant is structural, not a drift-prone
/// duplicated literal. The regression test `create_portal_conversation_assigns_personal_ownership`
/// asserts the runtime value; this const guards against a future edit
/// accidentally diverging one of the two call sites.
const PORTAL_VISIBILITY: crate::auth::principals::Visibility =
    crate::auth::principals::Visibility::Personal;

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct PortalSendRequest {
    agent_id: String,
    session_id: String,
    #[serde(default = "default_sender_name")]
    sender_name: String,
    message: String,
    /// IDs of pre-uploaded attachments to include with this message.
    #[serde(default)]
    attachment_ids: Vec<String>,
}

fn default_sender_name() -> String {
    "user".into()
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct PortalSendResponse {
    ok: bool,
}

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub(super) struct PortalHistoryQuery {
    agent_id: String,
    session_id: String,
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    100
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct PortalHistoryMessage {
    id: String,
    role: String,
    content: String,
}

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub(super) struct PortalConversationsQuery {
    agent_id: String,
    #[serde(default)]
    include_archived: bool,
    #[serde(default = "default_conversation_limit")]
    limit: i64,
}

fn default_conversation_limit() -> i64 {
    100
}

/// Portal conversation list row: the bare conversation summary plus a
/// `VisibilityTag` flattened into the same JSON object. Additive on the
/// wire (clients that ignore unknown fields continue to work; chip-aware
/// clients see the tag). Mirrors `MemoryListItem` / `TaskListItem` /
/// `WikiListItem` / `CronListItem`.
#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct PortalConversationListItem {
    #[serde(flatten)]
    summary: PortalConversationSummary,
    #[serde(flatten)]
    tag: crate::api::resources::VisibilityTag,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct PortalConversationsResponse {
    conversations: Vec<PortalConversationListItem>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct CreatePortalConversationRequest {
    agent_id: String,
    title: Option<String>,
    settings: Option<ConversationSettings>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct PortalConversationResponse {
    conversation: PortalConversation,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct UpdatePortalConversationRequest {
    agent_id: String,
    title: Option<String>,
    archived: Option<bool>,
    settings: Option<ConversationSettings>,
}

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub(super) struct DeletePortalConversationQuery {
    agent_id: String,
}

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub(super) struct ConversationDefaultsQuery {
    agent_id: String,
}

fn conversation_store(
    state: &Arc<ApiState>,
    agent_id: &str,
) -> Result<PortalConversationStore, StatusCode> {
    let pools = state.agent_pools.load();
    let pool = pools.get(agent_id).ok_or(StatusCode::NOT_FOUND)?;
    Ok(PortalConversationStore::new(pool.clone()))
}

/// Fire-and-forget message injection. The response arrives via the global SSE
/// event bus (`/api/events`), same as every other channel.
#[utoipa::path(
    post,
    path = "/portal/send",
    request_body = PortalSendRequest,
    responses(
        (status = 200, body = PortalSendResponse),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Agent not found"),
        (status = 503, description = "Messaging manager not available"),
    ),
    tag = "portal",
)]
pub(super) async fn portal_send(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    axum::Json(request): axum::Json<PortalSendRequest>,
) -> Result<Json<PortalSendResponse>, StatusCode> {
    let manager = state
        .messaging_manager
        .read()
        .await
        .clone()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    // Phase 4 authz gate: portal_send is a write path with an auto-create
    // branch. A single `get_ownership` probe up front decides which side
    // of the branch we take:
    //   * row present → existing conversation → `check_write`
    //   * row absent → brand-new session → `.await set_ownership` with
    //     `Visibility::Personal` AFTER `store.ensure` creates the row.
    //       Personal (never Org/Team): portal chats are private user
    //       data per phase-4 plan §12 A-2. Flipping this default is a
    //       tenant-wide data leak.
    //
    // The get_ownership/set_ownership pair is not a TOCTOU hazard: the
    // `resource_ownership` upsert collapses concurrent creates to one
    // row, and `store.ensure` is idempotent.
    let mut is_new_conversation = false;
    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let existing = crate::auth::repository::get_ownership(
            &pool,
            "portal_conversation",
            &request.session_id,
        )
        .await
        .map_err(|error| {
            tracing::warn!(
                %error,
                actor = %auth_ctx.principal_key(),
                resource_type = "portal_conversation",
                resource_id = %request.session_id,
                "authz get_ownership failed"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        if existing.is_some() {
            let access = crate::auth::check_write(
                &pool,
                &auth_ctx,
                "portal_conversation",
                &request.session_id,
            )
            .await
            .map_err(|error| {
                tracing::warn!(
                    %error,
                    actor = %auth_ctx.principal_key(),
                    resource_type = "portal_conversation",
                    resource_id = %request.session_id,
                    "authz check_write failed"
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            if !access.is_allowed() {
                crate::auth::policy::fire_denied_audit(
                    &state.audit,
                    &auth_ctx,
                    "portal_conversation",
                    request.session_id.as_str(),
                );
                return Err(access.to_status());
            }
        } else {
            is_new_conversation = true;
        }
    } else {
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["portal"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            session_id = %request.session_id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let store = conversation_store(&state, &request.agent_id)?;
    store
        .ensure(&request.agent_id, &request.session_id)
        .await
        .map_err(|error| {
            tracing::warn!(%error, session_id = %request.session_id, "failed to ensure portal conversation");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // A-12: `.await` set_ownership on the auto-create branch BEFORE
    // injecting the message, so a follow-up GET /portal/history from the
    // creator reads a consistent owner (no race window).
    if is_new_conversation && let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned()
    {
        crate::auth::repository::set_ownership(
            &pool,
            "portal_conversation",
            &request.session_id,
            None,
            &auth_ctx.principal_key(),
            PORTAL_VISIBILITY,
            None,
        )
        .await
        .map_err(|error| {
            tracing::error!(
                %error,
                session_id = %request.session_id,
                "failed to register portal_conversation ownership on auto-create"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }

    store
        .maybe_set_generated_title(&request.agent_id, &request.session_id, &request.message)
        .await
        .map_err(|error| {
            tracing::warn!(%error, session_id = %request.session_id, "failed to update generated portal title");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let conversation_id = request.session_id.clone();

    let mut metadata = HashMap::new();
    metadata.insert(
        "display_name".into(),
        serde_json::Value::String(request.sender_name.clone()),
    );

    // Resolve pre-uploaded attachments from saved_attachments table.
    let attachments: Vec<Attachment> = if request.attachment_ids.is_empty() {
        Vec::new()
    } else {
        use sqlx::Row as _;
        let pools = state.agent_pools.load();
        let pool = pools.get(&request.agent_id).ok_or(StatusCode::NOT_FOUND)?;

        let mut resolved = Vec::with_capacity(request.attachment_ids.len());
        let mut attachment_metas: Vec<crate::agent::channel_attachments::SavedAttachmentMeta> =
            Vec::with_capacity(request.attachment_ids.len());
        for attachment_id in &request.attachment_ids {
            // Filter by channel_id to prevent cross-conversation attachment
            // references: a user in conversation A should not be able to
            // reference an attachment uploaded in conversation B.
            let row = sqlx::query(
                "SELECT id, original_filename, saved_filename, mime_type, size_bytes \
                 FROM saved_attachments WHERE id = ? AND channel_id = ?",
            )
            .bind(attachment_id)
            .bind(&conversation_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| {
                tracing::warn!(%error, "failed to look up attachment");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

            if let Some(row) = row {
                let id: String = row.try_get("id").map_err(|error| {
                    tracing::error!(%error, "saved_attachments row missing id");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
                let filename: String = row.try_get("original_filename").map_err(|error| {
                    tracing::error!(%error, "saved_attachments row missing original_filename");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
                let saved_filename: String = row.try_get("saved_filename").map_err(|error| {
                    tracing::error!(%error, "saved_attachments row missing saved_filename");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
                let mime_type: String = row.try_get("mime_type").map_err(|error| {
                    tracing::error!(%error, "saved_attachments row missing mime_type");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
                let size_bytes = row
                    .try_get::<i64, _>("size_bytes")
                    .ok()
                    .and_then(|n| u64::try_from(n).ok())
                    .unwrap_or(0);
                attachment_metas.push(crate::agent::channel_attachments::SavedAttachmentMeta {
                    id: id.clone(),
                    filename: filename.clone(),
                    saved_filename,
                    mime_type: mime_type.clone(),
                    size_bytes,
                });
                resolved.push(Attachment {
                    filename,
                    mime_type,
                    url: String::new(),
                    size_bytes: Some(size_bytes),
                    auth_header: None,
                    pre_saved_id: Some(id),
                });
            }
        }
        if !attachment_metas.is_empty() {
            metadata.insert(
                "portal_attachment_metas".into(),
                serde_json::to_value(&attachment_metas).unwrap_or_default(),
            );
        }
        resolved
    };

    let content = if attachments.is_empty() {
        MessageContent::Text(request.message)
    } else {
        MessageContent::Media {
            text: Some(request.message),
            attachments,
        }
    };

    let inbound = InboundMessage {
        id: uuid::Uuid::new_v4().to_string(),
        source: "portal".into(),
        adapter: Some("portal".into()),
        conversation_id,
        sender_id: request.sender_name.clone(),
        agent_id: Some(request.agent_id.into()),
        content,
        timestamp: chrono::Utc::now(),
        metadata,
        formatted_author: Some(request.sender_name),
        auth_context: Some(auth_ctx),
    };

    manager.inject_message(inbound).await.map_err(|error| {
        tracing::warn!(%error, "failed to inject portal message");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(PortalSendResponse { ok: true }))
}

#[utoipa::path(
    get,
    path = "/portal/history",
    params(
        ("agent_id" = String, Query, description = "Agent ID"),
        ("session_id" = String, Query, description = "Session ID"),
        ("limit" = i64, Query, description = "Maximum number of messages to return (default: 100, max: 200)"),
    ),
    responses(
        (status = 200, body = Vec<PortalHistoryMessage>),
        (status = 404, description = "Agent not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "portal",
)]
pub(super) async fn portal_history(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Query(query): Query<PortalHistoryQuery>,
) -> Result<Json<Vec<PortalHistoryMessage>>, StatusCode> {
    // Phase 4 authz gate: read access to a portal conversation's message
    // history keys on the session_id (A-09 bare UUID). The `"portal"`
    // metric label matches the file resource family so
    // `spacebot_authz_skipped_total` cardinality stays flat. Pool-None is
    // the boot-window / startup-race case: always-on `tracing::error!`
    // plus a feature-gated counter.
    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let (access, admin_override) = crate::auth::check_read_with_audit(
            &pool,
            &auth_ctx,
            "portal_conversation",
            &query.session_id,
        )
        .await
        .map_err(|error| {
            tracing::warn!(
                %error,
                actor = %auth_ctx.principal_key(),
                resource_type = "portal_conversation",
                resource_id = %query.session_id,
                "authz check_read_with_audit failed"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        if !access.is_allowed() {
            crate::auth::policy::fire_denied_audit(
                &state.audit,
                &auth_ctx,
                "portal_conversation",
                query.session_id.as_str(),
            );
            return Err(access.to_status());
        }
        if admin_override {
            crate::auth::policy::fire_admin_read_audit(
                &state.audit,
                &auth_ctx,
                "portal_conversation",
                query.session_id.as_str(),
            );
        }
    } else {
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["portal"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            session_id = %query.session_id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let pools = state.agent_pools.load();
    let pool = pools.get(&query.agent_id).ok_or(StatusCode::NOT_FOUND)?;
    let logger = crate::conversation::ConversationLogger::new(pool.clone());

    let channel_id: crate::ChannelId = Arc::from(query.session_id.as_str());

    let messages = logger
        .load_recent(&channel_id, query.limit.min(200))
        .await
        .map_err(|error| {
            tracing::warn!(%error, "failed to load portal history");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let result: Vec<PortalHistoryMessage> = messages
        .into_iter()
        .map(|message| PortalHistoryMessage {
            id: message.id,
            role: message.role,
            content: message.content,
        })
        .collect();

    Ok(Json(result))
}

#[utoipa::path(
    get,
    path = "/portal/conversations",
    params(
        ("agent_id" = String, Query, description = "Agent ID"),
        ("include_archived" = bool, Query, description = "Include archived conversations"),
        ("limit" = i64, Query, description = "Maximum number of conversations to return (default: 100, max: 500)"),
    ),
    responses(
        (status = 200, body = PortalConversationsResponse),
        (status = 404, description = "Agent not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "portal",
)]
pub(super) async fn list_portal_conversations(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Query(query): Query<PortalConversationsQuery>,
) -> Result<Json<PortalConversationsResponse>, StatusCode> {
    // Phase 4 authz gate: scope the listing by the agent's read-access
    // row (same approach as `list_memories` / `list_tasks`). A list
    // without an `agent_id` filter cannot currently reach this handler
    // (the query type requires it). If that changes, a per-row gate is
    // the Phase-5 fix once the audit log lands.
    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let (access, admin_override) =
            crate::auth::check_read_with_audit(&pool, &auth_ctx, "agent", &query.agent_id)
                .await
                .map_err(|error| {
                    tracing::warn!(
                        %error,
                        actor = %auth_ctx.principal_key(),
                        resource_type = "agent",
                        resource_id = %query.agent_id,
                        "authz check_read_with_audit failed"
                    );
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
        if !access.is_allowed() {
            crate::auth::policy::fire_denied_audit(
                &state.audit,
                &auth_ctx,
                "agent",
                query.agent_id.as_str(),
            );
            return Err(access.to_status());
        }
        if admin_override {
            crate::auth::policy::fire_admin_read_audit(
                &state.audit,
                &auth_ctx,
                "agent",
                query.agent_id.as_str(),
            );
        }
    } else {
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["portal"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            agent_id = %query.agent_id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let store = conversation_store(&state, &query.agent_id)?;
    let summaries = store
        .list(&query.agent_id, query.include_archived, query.limit)
        .await
        .map_err(|error| {
            tracing::warn!(%error, agent_id = %query.agent_id, "failed to list portal conversations");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Batch-enrich visibility + team_name for the whole page in one
    // roundtrip against the instance pool. Mirrors the memory / task /
    // wiki / cron_job enrichment call sites. Resource type string
    // `"portal_conversation"` matches set_ownership in the create path
    // and check_write at the mutate path.
    let ids: Vec<String> = summaries.iter().map(|s| s.id.clone()).collect();
    let tags = if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        crate::api::resources::enrich_visibility_tags(&pool, "portal_conversation", &ids).await
    } else {
        tracing::warn!(
            handler = "portal",
            count = ids.len(),
            "enrichment skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
        std::collections::HashMap::new()
    };

    let conversations: Vec<PortalConversationListItem> = summaries
        .into_iter()
        .map(|summary| {
            let tag = tags.get(&summary.id).cloned().unwrap_or_default();
            PortalConversationListItem { summary, tag }
        })
        .collect();

    Ok(Json(PortalConversationsResponse { conversations }))
}

#[utoipa::path(
    post,
    path = "/portal/conversations",
    request_body = CreatePortalConversationRequest,
    responses(
        (status = 200, body = PortalConversationResponse),
        (status = 404, description = "Agent not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "portal",
)]
pub(super) async fn create_portal_conversation(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Json(request): Json<CreatePortalConversationRequest>,
) -> Result<Json<PortalConversationResponse>, StatusCode> {
    // Enforce that the caller can write to THIS agent before letting them
    // mint a conversation under it. Without this gate, any authenticated
    // user could POST with another user's `agent_id` and self-register as
    // owner of a conversation row they never had authority to create.
    // Mirrors the `"agent"` read-gate used by `list_portal_conversations`
    // — the same "can this caller reach this agent?" question, just on
    // the write side. Does not conflict with the subsequent Personal
    // set_ownership: the per-conversation ownership row still makes the
    // creator the owner, but now we've confirmed the creator had standing
    // to create under this agent in the first place.
    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let access = crate::auth::check_write(&pool, &auth_ctx, "agent", &request.agent_id)
            .await
            .map_err(|error| {
                tracing::warn!(
                    %error,
                    actor = %auth_ctx.principal_key(),
                    resource_type = "agent",
                    resource_id = %request.agent_id,
                    "authz check_write failed"
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if !access.is_allowed() {
            crate::auth::policy::fire_denied_audit(
                &state.audit,
                &auth_ctx,
                "agent",
                request.agent_id.as_str(),
            );
            return Err(access.to_status());
        }
    } else {
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["portal"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            agent_id = %request.agent_id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let store = conversation_store(&state, &request.agent_id)?;
    let conversation = store
        .create(&request.agent_id, request.title.as_deref(), request.settings)
        .await
        .map_err(|error| {
            tracing::warn!(%error, agent_id = %request.agent_id, "failed to create portal conversation");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // A-12: `.await` set_ownership (never `tokio::spawn`), otherwise a
    // create-then-read race would return 404 to the creator's next GET.
    // Visibility is `Personal` — phase-4 plan §12 A-2: portal chats are
    // private user data; defaulting to Org would leak this user's
    // conversations to the entire tenant. This is the single most
    // identity-sensitive table in the system.
    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        crate::auth::repository::set_ownership(
            &pool,
            "portal_conversation",
            &conversation.id,
            None,
            &auth_ctx.principal_key(),
            PORTAL_VISIBILITY,
            None,
        )
        .await
        .map_err(|error| {
            tracing::error!(
                %error,
                conversation_id = %conversation.id,
                "failed to register portal_conversation ownership"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    } else {
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            conversation_id = %conversation.id,
            "set_ownership skipped: instance_pool not attached"
        );
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["portal"])
            .inc();
    }

    Ok(Json(PortalConversationResponse { conversation }))
}

#[utoipa::path(
    put,
    path = "/portal/conversations/{session_id}",
    request_body = UpdatePortalConversationRequest,
    params(
        ("session_id" = String, Path, description = "Conversation session ID"),
    ),
    responses(
        (status = 200, body = PortalConversationResponse),
        (status = 404, description = "Conversation not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "portal",
)]
pub(super) async fn update_portal_conversation(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Path(session_id): Path<String>,
    Json(request): Json<UpdatePortalConversationRequest>,
) -> Result<Json<PortalConversationResponse>, StatusCode> {
    // Phase 4 authz gate: write on the conversation keyed by session_id
    // (A-09 bare UUID). NotYours → 404 per the hide-existence matrix.
    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let access = crate::auth::check_write(&pool, &auth_ctx, "portal_conversation", &session_id)
            .await
            .map_err(|error| {
                tracing::warn!(
                    %error,
                    actor = %auth_ctx.principal_key(),
                    resource_type = "portal_conversation",
                    resource_id = %session_id,
                    "authz check_write failed"
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if !access.is_allowed() {
            crate::auth::policy::fire_denied_audit(
                &state.audit,
                &auth_ctx,
                "portal_conversation",
                session_id.as_str(),
            );
            return Err(access.to_status());
        }
    } else {
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["portal"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            session_id = %session_id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let store = conversation_store(&state, &request.agent_id)?;
    let has_settings_update = request.settings.is_some();
    let conversation = store
        .update(
            &request.agent_id,
            &session_id,
            request.title.as_deref(),
            request.archived,
            request.settings,
        )
        .await
        .map_err(|error| {
            tracing::warn!(%error, %session_id, "failed to update portal conversation");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Notify the running channel to hot-reload its settings.
    if has_settings_update {
        let channel_states = state.channel_states.read().await;
        if let Some(channel_state) = channel_states.get(&session_id) {
            let _ = channel_state
                .deps
                .event_tx
                .send(crate::ProcessEvent::SettingsUpdated {
                    agent_id: channel_state.deps.agent_id.clone(),
                    channel_id: channel_state.channel_id.clone(),
                });
        }
    }

    Ok(Json(PortalConversationResponse { conversation }))
}

#[utoipa::path(
    delete,
    path = "/portal/conversations/{session_id}",
    params(
        ("session_id" = String, Path, description = "Conversation session ID"),
        ("agent_id" = String, Query, description = "Agent ID"),
    ),
    responses(
        (status = 200, body = PortalSendResponse),
        (status = 404, description = "Conversation not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "portal",
)]
pub(super) async fn delete_portal_conversation(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Path(session_id): Path<String>,
    Query(query): Query<DeletePortalConversationQuery>,
) -> Result<Json<PortalSendResponse>, StatusCode> {
    // Phase 4 authz gate: delete is a write on the conversation keyed by
    // session_id (A-09 bare UUID). Shares the inline `check_write` shape
    // with `update_portal_conversation`.
    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let access = crate::auth::check_write(&pool, &auth_ctx, "portal_conversation", &session_id)
            .await
            .map_err(|error| {
                tracing::warn!(
                    %error,
                    actor = %auth_ctx.principal_key(),
                    resource_type = "portal_conversation",
                    resource_id = %session_id,
                    "authz check_write failed"
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if !access.is_allowed() {
            crate::auth::policy::fire_denied_audit(
                &state.audit,
                &auth_ctx,
                "portal_conversation",
                session_id.as_str(),
            );
            return Err(access.to_status());
        }
    } else {
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["portal"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            session_id = %session_id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let store = conversation_store(&state, &query.agent_id)?;
    let deleted = store
        .delete(&query.agent_id, &session_id)
        .await
        .map_err(|error| {
            tracing::warn!(%error, %session_id, "failed to delete portal conversation");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !deleted {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(Json(PortalSendResponse { ok: true }))
}

/// Get conversation defaults for an agent.
/// Returns the resolved default settings and available options for new conversations.
#[utoipa::path(
    get,
    path = "/conversation-defaults",
    params(
        ("agent_id" = String, Query, description = "Agent ID"),
    ),
    responses(
        (status = 200, body = ConversationDefaultsResponse),
        (status = 404, description = "Agent not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "portal",
)]
pub(super) async fn conversation_defaults(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<ConversationDefaultsQuery>,
) -> Result<Json<ConversationDefaultsResponse>, StatusCode> {
    // Phase 4 authz gate: intentionally none. This is a catalog/config
    // endpoint. It returns an agent's default conversation settings and
    // the configured model list, neither of which expose per-user
    // conversation data. Gating here would require a caller-policy
    // contract the endpoint doesn't need; `existence of the agent` is
    // already disclosed by `/api/agents`.
    //
    // TODO(phase-5): if the payload grows to include per-user state
    // (per-agent history summaries, last-used settings), tighten to a
    // `check_read_with_audit("agent", &query.agent_id)` then.

    // Verify agent exists by checking agent_configs
    let agent_configs = state.agent_configs.load();
    let agent_exists = agent_configs.iter().any(|a| a.id == query.agent_id);
    if !agent_exists {
        return Err(StatusCode::NOT_FOUND);
    }

    // Resolve default model from agent's routing config.
    let default_model = {
        let runtime_configs = state.runtime_configs.load();
        runtime_configs
            .get(&query.agent_id)
            .map(|rc| rc.routing.load().channel.clone())
            .unwrap_or_else(|| "anthropic/claude-sonnet-4".to_string())
    };

    // Build available models from configured providers via the models catalog.
    let config_path = state.config_path.read().await.clone();
    let configured = super::models::configured_providers(&config_path).await;
    let catalog = super::models::ensure_models_cache().await;

    let available_models: Vec<ModelOption> = catalog
        .into_iter()
        .filter(|m| configured.contains(&m.provider.as_str()) && m.tool_call)
        .map(|m| ModelOption {
            id: m.id,
            name: m.name,
            provider: m.provider,
            context_window: m.context_window.unwrap_or(0) as usize,
            supports_tools: m.tool_call,
            supports_thinking: m.reasoning,
        })
        .collect();

    let response = ConversationDefaultsResponse {
        model: default_model,
        memory: MemoryMode::Full,
        delegation: DelegationMode::Standard,
        worker_context: WorkerContextMode::default(),
        available_models,
        memory_modes: vec!["full".to_string(), "ambient".to_string(), "off".to_string()],
        delegation_modes: vec!["standard".to_string(), "direct".to_string()],
        worker_history_modes: vec![
            "none".to_string(),
            "summary".to_string(),
            "recent".to_string(),
            "full".to_string(),
        ],
        worker_memory_modes: vec![
            "none".to_string(),
            "ambient".to_string(),
            "tools".to_string(),
            "full".to_string(),
        ],
    };

    Ok(Json(response))
}
