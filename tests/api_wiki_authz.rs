//! Phase 4 PR 2 T4.9 — handler-level authz tests for `src/api/wiki.rs`.
//!
//! Mirrors `tests/api_tasks_authz.rs`: exercises the full stack
//! (Entra middleware with MockValidator, handler
//! `check_read_with_audit` / `check_write`, policy module against a
//! real `resource_ownership` row). Wiki URLs key on `slug` but
//! ownership rows key on the UUID `page.id` (A-09). The handler
//! resolves slug → page via `load_by_slug` first, then gates on its
//! UUID.
//!
//! The policy module has 15 of its own tests in `tests/policy_table.rs`
//! so these tests stay tight: owner-200 + non-owner-404 + admin-bypass
//! on read, non-owner-404 on write, create-ownership, pool-None skip,
//! plus list-response chip-field presence (Phase 7 PR 3 Task 7.9).

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use spacebot::api::ApiState;
use spacebot::api::test_support::build_test_router_entra;
use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::principals::Visibility;
use spacebot::auth::repository::{
    get_ownership, set_ownership, upsert_team, upsert_user_from_auth,
};
use spacebot::auth::roles::{ROLE_ADMIN, ROLE_USER};
use spacebot::auth::testing::mint_mock_token;
use spacebot::wiki::{CreateWikiPageInput, WikiPage, WikiPageType, WikiStore};
use std::sync::Arc;
use tower::ServiceExt as _;

fn user_ctx(oid: &str, roles: Vec<&str>) -> AuthContext {
    AuthContext {
        principal_type: PrincipalType::User,
        tid: Arc::from("t1"),
        oid: Arc::from(oid),
        roles: roles.into_iter().map(Arc::from).collect(),
        groups: vec![],
        groups_overage: false,
        display_email: None,
        display_name: None,
    }
}

/// Install a wiki store on the test state using the same pool the
/// authz middleware reads. Required because `ApiState::new_test_state_*`
/// leaves `wiki_store` unset (handler returns 503 without it).
fn attach_wiki_store(state: &ApiState, pool: &sqlx::SqlitePool) {
    state.set_wiki_store(Arc::new(WikiStore::new(pool.clone())));
}

fn req_get_page(slug: &str, bearer: &str) -> Request<Body> {
    Request::builder()
        .uri(format!("/api/wiki/{slug}"))
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap()
}

fn req_edit_page(slug: &str, bearer: &str, body_json: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(format!("/api/wiki/{slug}/edit"))
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body_json.to_string()))
        .unwrap()
}

fn req_create_page(bearer: &str, title: &str) -> Request<Body> {
    let body = serde_json::json!({
        "title": title,
        "page_type": "concept",
        "content": "seed content",
    })
    .to_string();
    Request::builder()
        .method("POST")
        .uri("/api/wiki")
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap()
}

/// Create a wiki page directly via the store (bypassing the handler) and
/// register ownership against `owner`. Returns the created page.
async fn seed_page(pool: &sqlx::SqlitePool, owner: &AuthContext) -> WikiPage {
    let store = WikiStore::new(pool.clone());
    let page = store
        .create(CreateWikiPageInput {
            title: "Seed Page".to_string(),
            page_type: WikiPageType::Concept,
            content: "initial body".to_string(),
            related: vec![],
            author_type: "user".to_string(),
            author_id: "test".to_string(),
            edit_summary: None,
        })
        .await
        .unwrap();
    set_ownership(
        pool,
        "wiki_page",
        &page.id,
        None,
        &owner.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();
    page
}

#[tokio::test]
async fn non_owner_get_page_returns_404() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_wiki_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    let page = seed_page(&pool, &alice).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let res = app.oneshot(req_get_page(&page.slug, &token)).await.unwrap();

    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "non-owner on alice's personal wiki page must see 404 (hide existence), not 403"
    );
}

#[tokio::test]
async fn owner_get_page_returns_200() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_wiki_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    let page = seed_page(&pool, &alice).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let res = app.oneshot(req_get_page(&page.slug, &token)).await.unwrap();

    assert_eq!(
        res.status(),
        StatusCode::OK,
        "owner must see 200 on their own wiki page (got {:?})",
        res.status()
    );
}

#[tokio::test]
async fn admin_get_page_returns_200() {
    // Admin bypass: a SpacebotAdmin role skips per-resource ownership.
    // Regression guard against `is_admin` returning false on the wiki
    // handler gate.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_wiki_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let admin = user_ctx("admin-carol", vec![ROLE_ADMIN]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &admin).await.unwrap();
    let page = seed_page(&pool, &alice).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&admin);
    let res = app.oneshot(req_get_page(&page.slug, &token)).await.unwrap();

    assert_eq!(
        res.status(),
        StatusCode::OK,
        "admin must bypass per-resource ownership on GET /wiki/{{slug}}"
    );
}

#[tokio::test]
async fn non_owner_edit_page_returns_404() {
    // check_write returns DenyReason::NotYours for a wrong owner, which
    // `to_status` maps to 404 (same hide-existence policy as read). Proves
    // the write gate fires on edit_page; sibling write handlers
    // (restore_version, archive_page) share the same `check_write` block.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_wiki_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    let page = seed_page(&pool, &alice).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let body = r#"{"old_string":"initial body","new_string":"hijacked"}"#;
    let res = app
        .oneshot(req_edit_page(&page.slug, &token, body))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "non-owner edit on personal wiki page must see 404 per DenyReason::NotYours mapping"
    );
}

#[tokio::test]
async fn create_page_assigns_ownership() {
    // A-12: the POST /wiki handler MUST `.await` set_ownership before
    // returning. A tokio::spawn fire-and-forget would leave a window
    // where the creator's immediate GET /wiki/{slug} races into a
    // NotOwned 404. The proof is an ownership row present synchronously
    // after the POST completes.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_wiki_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let res = app
        .oneshot(req_create_page(&token, "Hello World"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "create_page must succeed");

    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let page_id = parsed["page"]["id"].as_str().expect("page.id").to_string();

    let own = get_ownership(&pool, "wiki_page", &page_id)
        .await
        .unwrap()
        .expect("ownership row must be present synchronously after POST");
    assert_eq!(
        own.owner_principal_key,
        alice.principal_key(),
        "owner principal_key must be the creator (alice)"
    );
    assert_eq!(own.visibility, "personal");
}

#[tokio::test]
async fn pool_none_skip_get_page() {
    // Regression guard for the early-startup / static-token fallback path.
    // When instance_pool is not attached, the get_page gate skips and the
    // handler proceeds; 503 here is expected because wiki_store is also
    // unset without an attached pool. Assertion: NOT 401/403, proving
    // the request passed auth + the no-op authz skip.
    let state = ApiState::new_test_state_with_mock_entra_no_pool();
    let bob = user_ctx("bob", vec![ROLE_USER]);

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let res = app
        .oneshot(req_get_page("ghost-page", &token))
        .await
        .unwrap();

    assert_ne!(
        res.status(),
        StatusCode::UNAUTHORIZED,
        "mock token must authenticate successfully"
    );
    assert_ne!(
        res.status(),
        StatusCode::FORBIDDEN,
        "authz skip must not cause a 403"
    );
}

#[tokio::test]
async fn list_pages_enriches_team_scoped_page_with_chip_fields() {
    // Phase 7 PR 3 Task 7.9 Step A. The SPA consumes `visibility` +
    // `team_name` on each list row to render the chip. This test pins
    // the wire shape so a regression to `Vec<WikiPageSummary>` (chip
    // absent) trips CI before the SPA notices a silent degradation.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_wiki_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    let team = upsert_team(&pool, "grp-platform", "Platform")
        .await
        .unwrap();
    let page = {
        let store = WikiStore::new(pool.clone());
        store
            .create(CreateWikiPageInput {
                title: "Team Runbook".to_string(),
                page_type: WikiPageType::Concept,
                content: "seed".to_string(),
                related: vec![],
                author_type: "user".to_string(),
                author_id: "alice".to_string(),
                edit_summary: None,
            })
            .await
            .unwrap()
    };
    set_ownership(
        &pool,
        "wiki_page",
        &page.id,
        None,
        &alice.principal_key(),
        Visibility::Team,
        Some(&team.id),
    )
    .await
    .unwrap();

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let req = Request::builder()
        .uri("/api/wiki")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(res.into_body(), 64 * 1024)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let pages = body["pages"].as_array().expect("pages array present");
    let row = pages
        .iter()
        .find(|p| p["id"] == page.id)
        .expect("seeded page present in list response");
    assert_eq!(
        row["visibility"].as_str(),
        Some("team"),
        "chip visibility field must be present on team-scoped page"
    );
    assert_eq!(
        row["team_name"].as_str(),
        Some("Platform"),
        "chip team_name must resolve to the team's display_name"
    );
    // Also confirm the flattened summary fields still cross the wire
    // (additive shape, not a rewrap).
    assert_eq!(row["slug"].as_str(), Some(page.slug.as_str()));
    assert_eq!(row["title"].as_str(), Some("Team Runbook"));
}
