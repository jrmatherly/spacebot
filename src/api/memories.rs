//! Memory-read HTTP handlers + their shared Phase-4 authz gate.
//!
//! All four read endpoints (`list_memories`, `search_memories`, `memory_graph`,
//! `memory_graph_neighbors`) consult `check_read_with_audit` with
//! `resource_type = "agent"` before touching the vector store or graph
//! index. Access keys on the agent's `resource_ownership` row (looked up
//! by `(resource_type="agent", resource_id=agent_id)`), not a per-memory
//! row. Memories belong to their agent, so agent-level ownership is the
//! correct authorization anchor; a per-memory row would be a write-time
//! fanout cost for a read-only semantic.
//!
//! The ~45-line gate block is inlined at each call site on purpose
//! (Phase 4 PR 2 decision N1 in
//! `.scratchpad/plans/entraid-auth/phase-4-authz-helpers.md`). A helper
//! would save writing but hurt grep-by-handler visibility during route
//! review. Since the block is only repeated within this file, drift
//! between copies is visible in a single-file diff; a reader grepping
//! any one handler sees the whole enforcement story without jumping.
//!
//! The metric label is always `"memories"` (the file's resource family),
//! never a per-handler sub-label. This keeps
//! `spacebot_authz_skipped_total` cardinality flat. Pool-None is treated
//! as a boot-window signal (always-on `tracing::error!` plus
//! feature-gated counter increment); a persistent non-zero rate after
//! startup is a startup-ordering regression worth paging.
//!
//! Phase 5 replaces the `tracing::info!` admin-override path with an
//! `AuditAppender::append` call against the hash-chained audit log.
//! Until that lands, the tracing log is the operational record.

use super::state::ApiState;

use crate::memory::search::{SearchConfig, SearchMode};
use crate::memory::types::{Association, Memory, MemorySearchResult, MemoryType};

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Resource-type key for memory ownership rows. Used by
/// `enrich_visibility_tags` at the list handler. Note that the authz
/// gates in this file key on `"agent"` (not memory) because memory reads
/// ride the agent's ownership row; the memory-specific resource-type
/// namespace only surfaces at enrichment time. Extracting the string to
/// a single constant prevents the BUG-C1 class of regression where a
/// future caller reaches for `"memories"` (the metric-label namespace,
/// plural) and silently breaks enrichment.
const MEMORY_RESOURCE_TYPE: &str = "memory";

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct MemoriesListResponse {
    memories: Vec<MemoryListItem>,
    total: usize,
}

/// Wrapper around `Memory` that carries Phase 7 enrichment fields inline
/// via `#[serde(flatten)]`. Additive on the wire: clients that ignore
/// unknown fields continue to work; chip-aware clients see the tag.
#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct MemoryListItem {
    #[serde(flatten)]
    memory: Memory,
    #[serde(flatten)]
    tag: crate::api::resources::VisibilityTag,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct MemoriesSearchResponse {
    results: Vec<MemorySearchResult>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct MemoryGraphResponse {
    nodes: Vec<Memory>,
    edges: Vec<Association>,
    total: usize,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct MemoryGraphNeighborsResponse {
    nodes: Vec<Memory>,
    edges: Vec<Association>,
}

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub(super) struct MemoriesListQuery {
    agent_id: String,
    #[serde(default = "default_memories_limit")]
    limit: i64,
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    memory_type: Option<String>,
    #[serde(default = "default_memories_sort")]
    sort: String,
}

fn default_memories_limit() -> i64 {
    50
}

pub(super) fn default_memories_sort() -> String {
    "recent".into()
}

pub(super) fn parse_sort(sort: &str) -> crate::memory::search::SearchSort {
    match sort {
        "importance" => crate::memory::search::SearchSort::Importance,
        "most_accessed" => crate::memory::search::SearchSort::MostAccessed,
        _ => crate::memory::search::SearchSort::Recent,
    }
}

pub(super) fn parse_memory_type(type_str: &str) -> Option<MemoryType> {
    match type_str {
        "fact" => Some(MemoryType::Fact),
        "preference" => Some(MemoryType::Preference),
        "decision" => Some(MemoryType::Decision),
        "identity" => Some(MemoryType::Identity),
        "event" => Some(MemoryType::Event),
        "observation" => Some(MemoryType::Observation),
        "goal" => Some(MemoryType::Goal),
        "todo" => Some(MemoryType::Todo),
        _ => None,
    }
}

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub(super) struct MemoriesSearchQuery {
    agent_id: String,
    q: String,
    #[serde(default = "default_search_limit")]
    limit: usize,
    #[serde(default)]
    memory_type: Option<String>,
}

fn default_search_limit() -> usize {
    20
}

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub(super) struct MemoryGraphQuery {
    agent_id: String,
    #[serde(default = "default_graph_limit")]
    limit: i64,
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    memory_type: Option<String>,
    #[serde(default = "default_memories_sort")]
    sort: String,
}

fn default_graph_limit() -> i64 {
    200
}

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub(super) struct MemoryGraphNeighborsQuery {
    agent_id: String,
    memory_id: String,
    #[serde(default = "default_neighbor_depth")]
    depth: u32,
    /// Comma-separated list of memory IDs the client already has.
    #[serde(default)]
    exclude: Option<String>,
}

fn default_neighbor_depth() -> u32 {
    1
}

/// List memories for an agent with sorting, filtering, and pagination.
#[utoipa::path(
    get,
    path = "/agents/memories",
    params(
        ("agent_id" = String, Query, description = "Agent ID"),
        ("limit" = i64, Query, description = "Maximum number of results to return (default 50, max 200)"),
        ("offset" = usize, Query, description = "Number of results to skip for pagination"),
        ("memory_type" = Option<String>, Query, description = "Filter by memory type (fact, preference, decision, identity, event, observation, goal, todo)"),
        ("sort" = String, Query, description = "Sort order: recent, importance, most_accessed (default: recent)"),
    ),
    responses(
        (status = 200, body = MemoriesListResponse),
        (status = 404, description = "Agent not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "memories",
)]
pub(super) async fn list_memories(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Query(query): Query<MemoriesListQuery>,
) -> Result<Json<MemoriesListResponse>, StatusCode> {
    // Phase 4 authz gate: read access to an agent's memories requires read
    // access to the agent resource itself. Admins and LegacyStatic principals
    // bypass (see `docs/design-docs/entra-role-permission-matrix.md`). Users
    // who aren't the owner AND can't reach the agent via team/org visibility
    // see 404 (matrix row: "Memory | read | no (404)" for non-owners).
    //
    // When the instance pool isn't attached yet (early startup window,
    // before `set_instance_pool` has run), the check is a no-op. The
    // always-on signal is the `tracing::warn!` below; the feature-gated
    // signal is `spacebot_authz_skipped_total{handler="memories"}` (only
    // compiled when the `metrics` feature is enabled; default builds
    // skip the counter and rely on the error log only). A persistent
    // non-zero warn rate (or counter rate) after startup indicates a
    // startup-ordering regression where the HTTP server is accepting
    // requests before the Phase 2 data model is attached.
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
        // Make the no-op path observable: the failure modes here (boot
        // window vs persistent misconfig vs startup race) are
        // indistinguishable at request time but very different at 100
        // qps. An alert on the counter rate distinguishes them.
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["memories"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            agent_id = %query.agent_id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let searches = state.memory_searches.load();
    let memory_search = searches.get(&query.agent_id).ok_or(StatusCode::NOT_FOUND)?;
    let store = memory_search.store();

    let limit = query.limit.min(200);
    let sort = parse_sort(&query.sort);
    let memory_type = query.memory_type.as_deref().and_then(parse_memory_type);

    let fetch_limit = limit + query.offset as i64;
    let all = store
        .get_sorted(sort, fetch_limit, memory_type)
        .await
        .map_err(|error| {
            tracing::warn!(%error, agent_id = %query.agent_id, "failed to list memories");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let total = all.len();
    let page: Vec<Memory> = all.into_iter().skip(query.offset).collect();

    // Phase 7 PR 1.5 Task 7.5a. Post-fetch enrichment for the per-agent
    // store: MemoryStore's pool is the per-agent memories.db while
    // resource_ownership + teams live in the instance pool, and SQLite
    // does not support cross-database JOIN. Batch-lookup against
    // state.instance_pool and attach visibility + team_name inline.
    let ids: Vec<String> = page.iter().map(|m| m.id.clone()).collect();
    let tags = if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        crate::api::resources::enrich_visibility_tags(&pool, MEMORY_RESOURCE_TYPE, &ids).await
    } else {
        // I4: match the authz-skipped observability pattern above. A
        // persistent pool-None on this path means enrichment is silently
        // degraded — chips absent across every memories list — and no
        // counter fires today. One grep-friendly message per handler so
        // "boot window" vs "startup-ordering bug" is distinguishable.
        tracing::warn!(
            handler = "memories",
            count = ids.len(),
            "enrichment skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
        std::collections::HashMap::new()
    };
    let memories: Vec<MemoryListItem> = page
        .into_iter()
        .map(|memory| {
            let tag = tags.get(&memory.id).cloned().unwrap_or_default();
            MemoryListItem { memory, tag }
        })
        .collect();

    Ok(Json(MemoriesListResponse { memories, total }))
}

/// Search memories using hybrid search (vector + FTS + graph).
#[utoipa::path(
    get,
    path = "/agents/memories/search",
    params(
        ("agent_id" = String, Query, description = "Agent ID"),
        ("q" = String, Query, description = "Search query string"),
        ("limit" = usize, Query, description = "Maximum number of results to return (default 20, max 100)"),
        ("memory_type" = Option<String>, Query, description = "Filter by memory type"),
    ),
    responses(
        (status = 200, body = MemoriesSearchResponse),
        (status = 404, description = "Agent not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "memories",
)]
pub(super) async fn search_memories(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Query(query): Query<MemoriesSearchQuery>,
) -> Result<Json<MemoriesSearchResponse>, StatusCode> {
    // Phase 4 authz gate: read access to an agent's memories requires read
    // access to the agent resource itself. Admins and LegacyStatic principals
    // bypass (see `docs/design-docs/entra-role-permission-matrix.md`). Users
    // who aren't the owner AND can't reach the agent via team/org visibility
    // see 404 (matrix row: "Memory | read | no (404)" for non-owners).
    //
    // When the instance pool isn't attached yet (early startup window,
    // before `set_instance_pool` has run), the check is a no-op. The
    // always-on signal is the `tracing::warn!` below; the feature-gated
    // signal is `spacebot_authz_skipped_total{handler="memories"}` (only
    // compiled when the `metrics` feature is enabled; default builds
    // skip the counter and rely on the error log only). A persistent
    // non-zero warn rate (or counter rate) after startup indicates a
    // startup-ordering regression where the HTTP server is accepting
    // requests before the Phase 2 data model is attached.
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
        // Make the no-op path observable: the failure modes here (boot
        // window vs persistent misconfig vs startup race) are
        // indistinguishable at request time but very different at 100
        // qps. An alert on the counter rate distinguishes them.
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["memories"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            agent_id = %query.agent_id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let searches = state.memory_searches.load();
    let memory_search = searches.get(&query.agent_id).ok_or(StatusCode::NOT_FOUND)?;

    let config = SearchConfig {
        mode: SearchMode::Hybrid,
        memory_type: query.memory_type.as_deref().and_then(parse_memory_type),
        max_results: query.limit.min(100),
        ..SearchConfig::default()
    };

    let results = memory_search.search(&query.q, &config)
        .await
        .map_err(|error| {
            tracing::warn!(%error, agent_id = %query.agent_id, query = %query.q, "memory search failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(MemoriesSearchResponse { results }))
}

/// Get a subgraph of memories: nodes + all edges between them.
#[utoipa::path(
    get,
    path = "/agents/memories/graph",
    params(
        ("agent_id" = String, Query, description = "Agent ID"),
        ("limit" = i64, Query, description = "Maximum number of nodes to return (default 200, max 500)"),
        ("offset" = usize, Query, description = "Number of nodes to skip for pagination"),
        ("memory_type" = Option<String>, Query, description = "Filter by memory type"),
        ("sort" = String, Query, description = "Sort order: recent, importance, most_accessed (default: recent)"),
    ),
    responses(
        (status = 200, body = MemoryGraphResponse),
        (status = 404, description = "Agent not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "memories",
)]
pub(super) async fn memory_graph(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Query(query): Query<MemoryGraphQuery>,
) -> Result<Json<MemoryGraphResponse>, StatusCode> {
    // Phase 4 authz gate: read access to an agent's memories requires read
    // access to the agent resource itself. Admins and LegacyStatic principals
    // bypass (see `docs/design-docs/entra-role-permission-matrix.md`). Users
    // who aren't the owner AND can't reach the agent via team/org visibility
    // see 404 (matrix row: "Memory | read | no (404)" for non-owners).
    //
    // When the instance pool isn't attached yet (early startup window,
    // before `set_instance_pool` has run), the check is a no-op. The
    // always-on signal is the `tracing::warn!` below; the feature-gated
    // signal is `spacebot_authz_skipped_total{handler="memories"}` (only
    // compiled when the `metrics` feature is enabled; default builds
    // skip the counter and rely on the error log only). A persistent
    // non-zero warn rate (or counter rate) after startup indicates a
    // startup-ordering regression where the HTTP server is accepting
    // requests before the Phase 2 data model is attached.
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
        // Make the no-op path observable: the failure modes here (boot
        // window vs persistent misconfig vs startup race) are
        // indistinguishable at request time but very different at 100
        // qps. An alert on the counter rate distinguishes them.
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["memories"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            agent_id = %query.agent_id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let searches = state.memory_searches.load();
    let memory_search = searches.get(&query.agent_id).ok_or(StatusCode::NOT_FOUND)?;
    let store = memory_search.store();

    let limit = query.limit.min(500);
    let sort = parse_sort(&query.sort);
    let memory_type = query.memory_type.as_deref().and_then(parse_memory_type);

    let fetch_limit = limit + query.offset as i64;
    let all = store
        .get_sorted(sort, fetch_limit, memory_type)
        .await
        .map_err(|error| {
            tracing::warn!(%error, agent_id = %query.agent_id, "failed to load graph nodes");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let total = all.len();
    let nodes: Vec<Memory> = all.into_iter().skip(query.offset).collect();
    let node_ids: Vec<String> = nodes.iter().map(|m| m.id.clone()).collect();

    let edges = store
        .get_associations_between(&node_ids)
        .await
        .map_err(|error| {
            tracing::warn!(%error, agent_id = %query.agent_id, "failed to load graph edges");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(MemoryGraphResponse {
        nodes,
        edges,
        total,
    }))
}

/// Get the neighbors of a specific memory node. Returns new nodes
/// and edges not already present in the client's graph.
#[utoipa::path(
    get,
    path = "/agents/memories/graph/neighbors",
    params(
        ("agent_id" = String, Query, description = "Agent ID"),
        ("memory_id" = String, Query, description = "Memory ID to get neighbors for"),
        ("depth" = u32, Query, description = "Neighbor traversal depth (default 1, max 3)"),
        ("exclude" = Option<String>, Query, description = "Comma-separated list of memory IDs to exclude from results"),
    ),
    responses(
        (status = 200, body = MemoryGraphNeighborsResponse),
        (status = 404, description = "Agent not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "memories",
)]
pub(super) async fn memory_graph_neighbors(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Query(query): Query<MemoryGraphNeighborsQuery>,
) -> Result<Json<MemoryGraphNeighborsResponse>, StatusCode> {
    // Phase 4 authz gate: read access to an agent's memories requires read
    // access to the agent resource itself. Admins and LegacyStatic principals
    // bypass (see `docs/design-docs/entra-role-permission-matrix.md`). Users
    // who aren't the owner AND can't reach the agent via team/org visibility
    // see 404 (matrix row: "Memory | read | no (404)" for non-owners).
    //
    // When the instance pool isn't attached yet (early startup window,
    // before `set_instance_pool` has run), the check is a no-op. The
    // always-on signal is the `tracing::warn!` below; the feature-gated
    // signal is `spacebot_authz_skipped_total{handler="memories"}` (only
    // compiled when the `metrics` feature is enabled; default builds
    // skip the counter and rely on the error log only). A persistent
    // non-zero warn rate (or counter rate) after startup indicates a
    // startup-ordering regression where the HTTP server is accepting
    // requests before the Phase 2 data model is attached.
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
        // Make the no-op path observable: the failure modes here (boot
        // window vs persistent misconfig vs startup race) are
        // indistinguishable at request time but very different at 100
        // qps. An alert on the counter rate distinguishes them.
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["memories"])
            .inc();
        tracing::error!(
            actor = %auth_ctx.principal_key(),
            agent_id = %query.agent_id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let searches = state.memory_searches.load();
    let memory_search = searches.get(&query.agent_id).ok_or(StatusCode::NOT_FOUND)?;
    let store = memory_search.store();

    let depth = query.depth.min(3);
    let exclude_ids: Vec<String> = query
        .exclude
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    let (nodes, edges) = store.get_neighbors(&query.memory_id, depth, &exclude_ids)
        .await
        .map_err(|error| {
            tracing::warn!(%error, agent_id = %query.agent_id, memory_id = %query.memory_id, "failed to load neighbors");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(MemoryGraphNeighborsResponse { nodes, edges }))
}

#[cfg(test)]
mod tests {
    use super::MemoryListItem;
    use crate::api::resources::VisibilityTag;
    use crate::auth::principals::Visibility;
    use crate::memory::types::{Memory, MemoryType};

    /// Serialized `MemoryListItem` must expose the union of `Memory` and
    /// `VisibilityTag` fields with no overwrites. `#[serde(flatten)]` on
    /// both members silently drops a key when names collide, which would
    /// mask a future `VisibilityTag` field rename or a new `Memory` field
    /// whose name happens to match an existing tag field. Mirrors
    /// `project_list_item_flatten_has_no_key_collision` in
    /// `src/api/projects.rs`.
    #[test]
    fn memory_list_item_flatten_has_no_key_collision() {
        let memory = Memory::new("test memory", MemoryType::Fact);
        let tag = VisibilityTag::new(Some(Visibility::Team), Some("Platform".into()));
        let item = MemoryListItem {
            memory: memory.clone(),
            tag,
        };
        let wrapper = serde_json::to_value(&item).expect("serialize MemoryListItem");
        let wrapper_keys: Vec<String> = wrapper
            .as_object()
            .expect("top-level object")
            .keys()
            .cloned()
            .collect();
        let memory_keys: Vec<String> = serde_json::to_value(&memory)
            .expect("serialize Memory")
            .as_object()
            .expect("memory object")
            .keys()
            .cloned()
            .collect();
        for key in &memory_keys {
            assert!(
                wrapper_keys.contains(key),
                "Memory field `{key}` was dropped by #[serde(flatten)] collision \
                 with VisibilityTag; wrapper keys: {wrapper_keys:?}"
            );
        }
        for tag_key in ["visibility", "team_name"] {
            assert!(
                !memory_keys.iter().any(|k| k == tag_key),
                "name collision: `{tag_key}` exists on both Memory and \
                 VisibilityTag; #[serde(flatten)] would silently drop one."
            );
        }
        assert_eq!(
            wrapper_keys.len(),
            memory_keys.len() + 2,
            "wrapper key count should be Memory fields + 2 VisibilityTag fields; \
             got {} expected {}. Keys: {:?}",
            wrapper_keys.len(),
            memory_keys.len() + 2,
            wrapper_keys
        );
    }
}
