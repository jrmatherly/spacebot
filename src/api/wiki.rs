//! Wiki HTTP handlers + shared Phase-4 authz gate.
//!
//! All per-page read + write endpoints consult `check_read_with_audit`
//! / `check_write` with `resource_type = "wiki_page"` before returning
//! or mutating page state. Access keys on the page's UUID `id` (A-09:
//! bare UUID, never the slug or any sigil'd variant), even though the
//! URL path uses the human-friendly `slug`. Per-page handlers therefore
//! resolve slug → page by calling `WikiStore::load_by_slug` before the
//! gate; that fetch is also the "does the page exist" 404 check the
//! handler would have to make anyway. Missing-page 404 and `NotOwned`
//! 404 collapse to the same client-visible shape.
//!
//! `list_pages` and `search_pages` carry a Phase-5 TODO for the
//! unfiltered-list case: they currently return every non-archived page
//! the instance holds to any authenticated caller. Wiki listings are
//! generally less sensitive than tasks or memories (wiki content is
//! team-shared context by convention), but the correct fix is the same
//! as `list_tasks` — per-row `check_read` once the audit log lands.
//!
//! `create_page` skips the pre-check (nothing exists yet) and `.await`s
//! `set_ownership("wiki_page", &page.id, ...)` AFTER the insert
//! succeeds. The `.await` is load-bearing (A-12): a `tokio::spawn`
//! fire-and-forget here races the creator's subsequent GET
//! /wiki/{slug} into a 404.
//!
//! The ~45-line inline gate block mirrors `src/api/memories.rs` and
//! `src/api/tasks.rs` per Phase 4 PR 2 decision N1: single-file
//! grep-visibility beats DRY. Pool-None is always-on `tracing::warn!`
//! plus feature-gated `spacebot_authz_skipped_total{handler="wiki"}`.
//! The metric label is the file resource family (`"wiki"`), never a
//! per-handler sub-label, which keeps cardinality flat.
//!
//! Deferred perf: `edit_page` / `restore_version` / `archive_page`
//! each call `load_by_slug` for the gate and then call a `WikiStore`
//! write method that re-fetches internally. This is a single indexed
//! SELECT and wiki edits are a cold path relative to task writes, so
//! the duplicate is accepted rather than mirroring `tasks.rs`'s
//! `update_prefetched` optimization. Revisit if the wiki write path
//! ever lands on the hot path.

use super::state::ApiState;
use crate::error::{Error as CrateError, WikiError};
use crate::wiki::{CreateWikiPageInput, EditWikiPageInput, WikiPageType, WikiStore};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Map a crate-level wiki error to an HTTP status.
fn wiki_error_status(error: CrateError) -> StatusCode {
    match error {
        CrateError::Wiki(wiki) => match *wiki {
            WikiError::NotFound { .. } | WikiError::VersionNotFound { .. } => StatusCode::NOT_FOUND,
            WikiError::EditFailed(_) => StatusCode::BAD_REQUEST,
            WikiError::Database(inner) => {
                tracing::error!(%inner, "wiki database error");
                StatusCode::INTERNAL_SERVER_ERROR
            }
            WikiError::Other(inner) => {
                tracing::error!(%inner, "wiki store error");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        },
        other => {
            tracing::error!(error = %other, "wiki handler error");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub(super) struct WikiListQuery {
    #[serde(default)]
    page_type: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub(super) struct WikiSearchQuery {
    query: String,
    #[serde(default)]
    page_type: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub(super) struct WikiHistoryQuery {
    #[serde(default = "default_history_limit")]
    limit: i64,
}

fn default_history_limit() -> i64 {
    20
}

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub(super) struct WikiVersionQuery {
    #[serde(default)]
    version: Option<i64>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct CreatePageRequest {
    title: String,
    page_type: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    related: Vec<String>,
    #[serde(default)]
    edit_summary: Option<String>,
    /// Who is creating this page: agent_id or user identifier.
    #[serde(default = "default_author")]
    author_id: String,
    #[serde(default = "default_author_type")]
    author_type: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct EditPageRequest {
    old_string: String,
    new_string: String,
    #[serde(default)]
    replace_all: bool,
    #[serde(default)]
    edit_summary: Option<String>,
    #[serde(default = "default_author")]
    author_id: String,
    #[serde(default = "default_author_type")]
    author_type: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct RestoreVersionRequest {
    version: i64,
    #[serde(default = "default_author")]
    author_id: String,
    #[serde(default = "default_author_type")]
    author_type: String,
}

fn default_author() -> String {
    "user".to_string()
}

fn default_author_type() -> String {
    "user".to_string()
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct WikiListResponse {
    pages: Vec<crate::wiki::WikiPageSummary>,
    total: usize,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct WikiPageResponse {
    page: crate::wiki::WikiPage,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct WikiHistoryResponse {
    versions: Vec<crate::wiki::WikiPageVersion>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct WikiActionResponse {
    success: bool,
    message: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn get_wiki_store(state: &ApiState) -> Result<Arc<WikiStore>, StatusCode> {
    state
        .wiki_store
        .load()
        .as_ref()
        .clone()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)
}

fn parse_page_type(s: Option<&str>) -> Result<Option<WikiPageType>, StatusCode> {
    match s {
        None => Ok(None),
        Some(v) => Ok(Some(WikiPageType::parse(v).ok_or(StatusCode::BAD_REQUEST)?)),
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /wiki — list all wiki pages
#[utoipa::path(
    get,
    path = "/wiki",
    params(WikiListQuery),
    responses(
        (status = 200, body = WikiListResponse),
        (status = 503, description = "Wiki store not initialized"),
    ),
    tag = "wiki",
)]
pub(super) async fn list_pages(
    State(state): State<Arc<ApiState>>,
    _auth_ctx: crate::auth::context::AuthContext,
    Query(query): Query<WikiListQuery>,
) -> Result<Json<WikiListResponse>, StatusCode> {
    // TODO(phase-5): gate this unfiltered listing path (currently returns
    // every non-archived page the instance holds to any authenticated
    // caller). Wiki content is generally team-shared by convention, so
    // the exposure is milder than `list_tasks`, but the correct fix is
    // per-row `check_read` once the audit log lands and can absorb the
    // N+1 emission cost.
    let store = get_wiki_store(&state)?;
    let page_type = parse_page_type(query.page_type.as_deref())?;
    let pages = store.list(page_type).await.map_err(wiki_error_status)?;
    let total = pages.len();
    Ok(Json(WikiListResponse { pages, total }))
}

/// GET /wiki/search — search wiki pages
#[utoipa::path(
    get,
    path = "/wiki/search",
    params(WikiSearchQuery),
    responses(
        (status = 200, body = WikiListResponse),
        (status = 503, description = "Wiki store not initialized"),
    ),
    tag = "wiki",
)]
pub(super) async fn search_pages(
    State(state): State<Arc<ApiState>>,
    _auth_ctx: crate::auth::context::AuthContext,
    Query(query): Query<WikiSearchQuery>,
) -> Result<Json<WikiListResponse>, StatusCode> {
    // TODO(phase-5): same Phase-5 TODO as `list_pages` — unfiltered
    // search results are returned to any authenticated caller today;
    // per-row `check_read` post-filter is the planned fix.
    let store = get_wiki_store(&state)?;
    let page_type = parse_page_type(query.page_type.as_deref())?;
    let pages = store
        .search(&query.query, page_type)
        .await
        .map_err(wiki_error_status)?;
    let total = pages.len();
    Ok(Json(WikiListResponse { pages, total }))
}

/// POST /wiki — create a new wiki page
#[utoipa::path(
    post,
    path = "/wiki",
    request_body = CreatePageRequest,
    responses(
        (status = 200, body = WikiPageResponse),
        (status = 400, description = "Invalid page_type"),
        (status = 503, description = "Wiki store not initialized"),
    ),
    tag = "wiki",
)]
pub(super) async fn create_page(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Json(request): Json<CreatePageRequest>,
) -> Result<Json<WikiPageResponse>, StatusCode> {
    let store = get_wiki_store(&state)?;
    let page_type = WikiPageType::parse(&request.page_type).ok_or(StatusCode::BAD_REQUEST)?;

    let page = store
        .create(CreateWikiPageInput {
            title: request.title,
            page_type,
            content: request.content,
            related: request.related,
            author_type: request.author_type,
            author_id: request.author_id,
            edit_summary: request.edit_summary,
        })
        .await
        .map_err(wiki_error_status)?;

    // A-12: `.await` set_ownership — a fire-and-forget `tokio::spawn` here
    // races the creator's subsequent GET /wiki/{slug} into a 404.
    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        crate::auth::repository::set_ownership(
            &pool,
            "wiki_page",
            &page.id,
            None,
            &auth_ctx.principal_key(),
            crate::auth::principals::Visibility::Personal,
            None,
        )
        .await
        .map_err(|error| {
            tracing::error!(%error, page_id = %page.id, "failed to register wiki_page ownership");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    } else {
        tracing::warn!(
            actor = %auth_ctx.principal_key(),
            page_id = %page.id,
            "set_ownership skipped: instance_pool not attached"
        );
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["wiki"])
            .inc();
    }

    Ok(Json(WikiPageResponse { page }))
}

/// GET /wiki/:slug — read a wiki page
#[utoipa::path(
    get,
    path = "/wiki/{slug}",
    params(
        ("slug" = String, Path, description = "Page slug"),
        WikiVersionQuery,
    ),
    responses(
        (status = 200, body = WikiPageResponse),
        (status = 404, description = "Page not found"),
        (status = 503, description = "Wiki store not initialized"),
    ),
    tag = "wiki",
)]
pub(super) async fn get_page(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Path(slug): Path<String>,
    Query(query): Query<WikiVersionQuery>,
) -> Result<Json<WikiPageResponse>, StatusCode> {
    let store = get_wiki_store(&state)?;

    // Fetch before the authz gate: URL path is slug, but ownership rows
    // key on page.id (UUID) per A-09. Missing-page 404 and NotOwned 404
    // collapse to the same client-visible shape. When a specific version
    // was requested, we still gate on the current page's ownership and
    // then overlay the historical content below.
    let page = store
        .read(&slug, query.version)
        .await
        .map_err(wiki_error_status)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let (access, admin_override) =
            crate::auth::check_read_with_audit(&pool, &auth_ctx, "wiki_page", &page.id)
                .await
                .map_err(|error| {
                    tracing::warn!(
                        %error,
                        actor = %auth_ctx.principal_key(),
                        resource_type = "wiki_page",
                        resource_id = %page.id,
                        "authz check_read_with_audit failed"
                    );
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
        if !access.is_allowed() {
            return Err(access.to_status());
        }
        if admin_override {
            tracing::info!(
                actor = %auth_ctx.principal_key(),
                resource_type = "wiki_page",
                resource_id = %page.id,
                "admin_read override (audit event queued for Phase 5)"
            );
        }
    } else {
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["wiki"])
            .inc();
        tracing::warn!(
            actor = %auth_ctx.principal_key(),
            page_id = %page.id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    Ok(Json(WikiPageResponse { page }))
}

/// POST /wiki/:slug/edit — apply a partial edit
#[utoipa::path(
    post,
    path = "/wiki/{slug}/edit",
    params(("slug" = String, Path, description = "Page slug")),
    request_body = EditPageRequest,
    responses(
        (status = 200, body = WikiPageResponse),
        (status = 400, description = "Edit match failed"),
        (status = 404, description = "Page not found"),
        (status = 503, description = "Wiki store not initialized"),
    ),
    tag = "wiki",
)]
pub(super) async fn edit_page(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Path(slug): Path<String>,
    Json(request): Json<EditPageRequest>,
) -> Result<Json<WikiPageResponse>, StatusCode> {
    let store = get_wiki_store(&state)?;

    // Fetch-before-gate: URL path is slug, ownership keys on page.id
    // (UUID). Missing-page 404 and NotOwned 404 are the same
    // client-visible shape.
    let existing = store
        .load_by_slug(&slug)
        .await
        .map_err(wiki_error_status)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let access = crate::auth::check_write(&pool, &auth_ctx, "wiki_page", &existing.id)
            .await
            .map_err(|error| {
                tracing::warn!(
                    %error,
                    actor = %auth_ctx.principal_key(),
                    resource_type = "wiki_page",
                    resource_id = %existing.id,
                    "authz check_write failed"
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if !access.is_allowed() {
            return Err(access.to_status());
        }
    } else {
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["wiki"])
            .inc();
        tracing::warn!(
            actor = %auth_ctx.principal_key(),
            page_id = %existing.id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let page = store
        .edit(EditWikiPageInput {
            slug,
            old_string: request.old_string,
            new_string: request.new_string,
            replace_all: request.replace_all,
            edit_summary: request.edit_summary,
            author_type: request.author_type,
            author_id: request.author_id,
        })
        .await
        .map_err(wiki_error_status)?;
    Ok(Json(WikiPageResponse { page }))
}

/// GET /wiki/:slug/history — list version history
#[utoipa::path(
    get,
    path = "/wiki/{slug}/history",
    params(
        ("slug" = String, Path, description = "Page slug"),
        WikiHistoryQuery,
    ),
    responses(
        (status = 200, body = WikiHistoryResponse),
        (status = 503, description = "Wiki store not initialized"),
    ),
    tag = "wiki",
)]
pub(super) async fn get_history(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Path(slug): Path<String>,
    Query(query): Query<WikiHistoryQuery>,
) -> Result<Json<WikiHistoryResponse>, StatusCode> {
    let store = get_wiki_store(&state)?;

    // History disclosure is existence disclosure: gate as a read against
    // the current page UUID. Missing-page 404 and NotOwned 404 collapse.
    let existing = store
        .load_by_slug(&slug)
        .await
        .map_err(wiki_error_status)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let (access, admin_override) =
            crate::auth::check_read_with_audit(&pool, &auth_ctx, "wiki_page", &existing.id)
                .await
                .map_err(|error| {
                    tracing::warn!(
                        %error,
                        actor = %auth_ctx.principal_key(),
                        resource_type = "wiki_page",
                        resource_id = %existing.id,
                        "authz check_read_with_audit failed"
                    );
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
        if !access.is_allowed() {
            return Err(access.to_status());
        }
        if admin_override {
            tracing::info!(
                actor = %auth_ctx.principal_key(),
                resource_type = "wiki_page",
                resource_id = %existing.id,
                "admin_read override (audit event queued for Phase 5)"
            );
        }
    } else {
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["wiki"])
            .inc();
        tracing::warn!(
            actor = %auth_ctx.principal_key(),
            page_id = %existing.id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let versions = store
        .history(&slug, query.limit)
        .await
        .map_err(wiki_error_status)?;
    Ok(Json(WikiHistoryResponse { versions }))
}

/// POST /wiki/:slug/restore — restore to a historical version
#[utoipa::path(
    post,
    path = "/wiki/{slug}/restore",
    params(("slug" = String, Path, description = "Page slug")),
    request_body = RestoreVersionRequest,
    responses(
        (status = 200, body = WikiPageResponse),
        (status = 404, description = "Page or version not found"),
        (status = 503, description = "Wiki store not initialized"),
    ),
    tag = "wiki",
)]
pub(super) async fn restore_version(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Path(slug): Path<String>,
    Json(request): Json<RestoreVersionRequest>,
) -> Result<Json<WikiPageResponse>, StatusCode> {
    let store = get_wiki_store(&state)?;

    // Fetch-before-gate: URL path is slug, ownership keys on page.id
    // (UUID). Missing-page 404 and NotOwned 404 collapse.
    let existing = store
        .load_by_slug(&slug)
        .await
        .map_err(wiki_error_status)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let access = crate::auth::check_write(&pool, &auth_ctx, "wiki_page", &existing.id)
            .await
            .map_err(|error| {
                tracing::warn!(
                    %error,
                    actor = %auth_ctx.principal_key(),
                    resource_type = "wiki_page",
                    resource_id = %existing.id,
                    "authz check_write failed"
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if !access.is_allowed() {
            return Err(access.to_status());
        }
    } else {
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["wiki"])
            .inc();
        tracing::warn!(
            actor = %auth_ctx.principal_key(),
            page_id = %existing.id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    let page = store
        .restore(
            &slug,
            request.version,
            &request.author_type,
            &request.author_id,
        )
        .await
        .map_err(wiki_error_status)?;
    Ok(Json(WikiPageResponse { page }))
}

/// DELETE /wiki/:slug — archive a page
#[utoipa::path(
    delete,
    path = "/wiki/{slug}",
    params(("slug" = String, Path, description = "Page slug")),
    responses(
        (status = 200, body = WikiActionResponse),
        (status = 503, description = "Wiki store not initialized"),
    ),
    tag = "wiki",
)]
pub(super) async fn archive_page(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Path(slug): Path<String>,
) -> Result<Json<WikiActionResponse>, StatusCode> {
    let store = get_wiki_store(&state)?;

    // Fetch-before-gate: URL path is slug, ownership keys on page.id
    // (UUID). Missing-page 404 and NotOwned 404 collapse.
    let existing = store
        .load_by_slug(&slug)
        .await
        .map_err(wiki_error_status)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
        let access = crate::auth::check_write(&pool, &auth_ctx, "wiki_page", &existing.id)
            .await
            .map_err(|error| {
                tracing::warn!(
                    %error,
                    actor = %auth_ctx.principal_key(),
                    resource_type = "wiki_page",
                    resource_id = %existing.id,
                    "authz check_write failed"
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if !access.is_allowed() {
            return Err(access.to_status());
        }
    } else {
        #[cfg(feature = "metrics")]
        crate::telemetry::Metrics::global()
            .authz_skipped_total
            .with_label_values(&["wiki"])
            .inc();
        tracing::warn!(
            actor = %auth_ctx.principal_key(),
            page_id = %existing.id,
            "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
        );
    }

    store.archive(&slug).await.map_err(wiki_error_status)?;
    Ok(Json(WikiActionResponse {
        success: true,
        message: format!("Page '{slug}' archived"),
    }))
}
