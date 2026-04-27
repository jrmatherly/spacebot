//! Phase 4 PR 2 T4.13b — handler-level authz tests for `src/api/projects.rs`.
//!
//! Mirrors `tests/api_tasks_authz.rs`: exercises the full stack (Entra
//! middleware with MockValidator, handler `check_read_with_audit` /
//! `check_write`, policy module against a real `resource_ownership`
//! row). Thirteen project handlers share a single ~45-line inline gate;
//! non-owner GET + owner-200 + admin bypass + create-ownership +
//! pool-None skip are enough to prove the gate is wired without
//! re-covering the policy module, which has its own coverage in
//! `tests/policy_table.rs`.
//!
//! Project URLs key on the bare project UUID (A-09) and so do
//! ownership rows; there is no fetch-before-gate indirection on
//! per-project handlers. The create handler `.await`s `set_ownership`
//! (A-12) so a caller's immediate follow-up GET sees the new project.

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
use spacebot::projects::ProjectStore;
use spacebot::projects::store::CreateProjectInput;
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

/// Install a project store on the test state using the same pool the
/// authz middleware reads. Required because `ApiState::new_test_state_*`
/// leaves `project_store` unset (service returns 404 without it).
fn attach_project_store(state: &ApiState, pool: &sqlx::SqlitePool) {
    state.set_project_store(Arc::new(ProjectStore::new(Arc::new(
        spacebot::db::DbPool::Sqlite(pool.clone()),
    ))));
}

fn req_get_project(project_id: &str, bearer: &str) -> Request<Body> {
    Request::builder()
        .uri(format!("/api/agents/projects/{project_id}"))
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap()
}

fn req_create_project(bearer: &str, name: &str, root_path: &str) -> Request<Body> {
    let body = serde_json::json!({
        "name": name,
        "root_path": root_path,
        // auto_discover=false so we skip the background git/disk scan
        // which pokes at the filesystem — unit tests should not.
        "auto_discover": false,
    })
    .to_string();
    Request::builder()
        .method("POST")
        .uri("/api/agents/projects")
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap()
}

/// Create a project directly via the store (bypassing the handler) and
/// register ownership against `owner`. Returns the project's UUID.
async fn seed_project(pool: &sqlx::SqlitePool, owner: &AuthContext) -> String {
    let store = ProjectStore::new(Arc::new(spacebot::db::DbPool::Sqlite(pool.clone())));
    let project = store
        .create_project(CreateProjectInput {
            name: "seeded".to_string(),
            description: String::new(),
            icon: String::new(),
            tags: vec![],
            root_path: "/tmp/seeded-proj".to_string(),
            settings: serde_json::json!({}),
        })
        .await
        .unwrap();
    set_ownership(
        pool,
        "project",
        &project.id,
        None,
        &owner.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();
    project.id
}

#[tokio::test]
async fn non_owner_get_project_returns_404() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_project_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    let project_id = seed_project(&pool, &alice).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let res = app
        .oneshot(req_get_project(&project_id, &token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "non-owner on alice's personal project must see 404 (hide existence), not 403"
    );
}

#[tokio::test]
async fn owner_get_project_returns_200() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_project_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    let project_id = seed_project(&pool, &alice).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let res = app
        .oneshot(req_get_project(&project_id, &token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::OK,
        "owner must see 200 on their own project (got {:?})",
        res.status()
    );
}

#[tokio::test]
async fn admin_bypass_project_read() {
    // Admin bypass: a SpacebotAdmin role skips per-resource ownership.
    // Regression guard against `is_admin` returning false on the
    // projects handler gate.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_project_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let admin = user_ctx("admin-carol", vec![ROLE_ADMIN]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &admin).await.unwrap();
    let project_id = seed_project(&pool, &alice).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&admin);
    let res = app
        .oneshot(req_get_project(&project_id, &token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::OK,
        "admin must bypass per-resource ownership on GET /agents/projects/{{id}}"
    );
}

#[tokio::test]
async fn create_project_assigns_ownership() {
    // A-12: the POST /agents/projects handler MUST `.await` set_ownership
    // before returning. A tokio::spawn fire-and-forget would leave a
    // window where the creator's immediate GET /agents/projects/{id}
    // races into a NotOwned 404. The proof is an ownership row present
    // synchronously after the POST completes.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_project_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    // Use a root_path that does NOT exist on disk; the handler's
    // background discovery spawn is keyed on `root.is_dir()` and
    // returns early for missing paths, so the test stays hermetic.
    let res = app
        .oneshot(req_create_project(
            &token,
            "my-proj",
            "/nonexistent/path-for-authz-test",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "create_project must succeed");

    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let project_id = parsed["id"].as_str().expect("project.id").to_string();

    let own = get_ownership(&pool, "project", &project_id)
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
async fn pool_none_skip_get_project() {
    // Regression guard for the early-startup / static-token fallback
    // path. When instance_pool is not attached, the get_project gate
    // skips and the handler proceeds. The handler returns 404 because
    // project_store is also unset without an attached pool.
    // Assertion: NOT 401/403, proving the request passed auth + the
    // no-op authz skip.
    let state = ApiState::new_test_state_with_mock_entra_no_pool();
    let bob = user_ctx("bob", vec![ROLE_USER]);

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let res = app
        .oneshot(req_get_project("any-project-id", &token))
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
async fn list_projects_enriches_team_scoped_project_with_chip_fields() {
    // The SPA consumes `visibility` + `team_name` on each list row to
    // render the chip. Pins the wire shape so a regression to bare
    // `Vec<Project>` (chip absent) trips CI before the SPA notices a
    // silent degradation. Mirrors
    // tests/api_wiki_authz.rs::list_pages_enriches_team_scoped_page_with_chip_fields.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_project_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    let team = upsert_team(&pool, "grp-platform", "Platform")
        .await
        .unwrap();
    let project = {
        let store = ProjectStore::new(Arc::new(spacebot::db::DbPool::Sqlite(pool.clone())));
        store
            .create_project(CreateProjectInput {
                name: "Team Runbook".to_string(),
                description: String::new(),
                icon: String::new(),
                tags: vec![],
                root_path: "/tmp/team-runbook".to_string(),
                settings: serde_json::json!({}),
            })
            .await
            .unwrap()
    };
    set_ownership(
        &pool,
        "project",
        &project.id,
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
        .uri("/api/agents/projects")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(res.into_body(), 64 * 1024)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let projects = body["projects"].as_array().expect("projects array present");
    let row = projects
        .iter()
        .find(|p| p["id"] == project.id)
        .expect("seeded project present in list response");
    assert_eq!(
        row["visibility"].as_str(),
        Some("team"),
        "chip visibility field must be present on team-scoped project"
    );
    assert_eq!(
        row["team_name"].as_str(),
        Some("Platform"),
        "chip team_name must resolve to the team's display_name"
    );
    // Flattened inner-Project fields still cross the wire (additive shape).
    assert_eq!(row["name"].as_str(), Some("Team Runbook"));
    assert_eq!(row["root_path"].as_str(), Some("/tmp/team-runbook"));
}

// Scope-filter tests for GET /api/agents/projects?scope=mine|team|org.
// Projects come from ProjectStore (SQL-backed), and scope filtering
// applies a post-fetch allowlist drawn from resource_ownership +
// team_memberships. Same five cases as list_agents: Mine, Team, Org,
// no-scope, admin-bypass.

async fn seed_owned_project(
    pool: &sqlx::SqlitePool,
    owner: &AuthContext,
    name: &str,
    root_path: &str,
) -> String {
    let store = ProjectStore::new(Arc::new(spacebot::db::DbPool::Sqlite(pool.clone())));
    let project = store
        .create_project(CreateProjectInput {
            name: name.to_string(),
            description: String::new(),
            icon: String::new(),
            tags: vec![],
            root_path: root_path.to_string(),
            settings: serde_json::json!({}),
        })
        .await
        .unwrap();
    set_ownership(
        pool,
        "project",
        &project.id,
        None,
        &owner.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();
    project.id
}

async fn fetch_projects(app: axum::Router, token: &str, scope_query: Option<&str>) -> Vec<String> {
    let uri = match scope_query {
        Some(s) => format!("/api/agents/projects?scope={s}"),
        None => "/api/agents/projects".to_string(),
    };
    let res = app
        .oneshot(
            Request::builder()
                .uri(uri)
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    body["projects"]
        .as_array()
        .expect("projects array")
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn list_projects_scope_mine_returns_only_owned_projects() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_project_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    let alice_proj = seed_owned_project(&pool, &alice, "alice-proj", "/tmp/alice-p").await;
    let _bob_proj = seed_owned_project(&pool, &bob, "bob-proj", "/tmp/bob-p").await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let mut ids = fetch_projects(app, &token, Some("mine")).await;
    ids.sort();
    assert_eq!(
        ids,
        vec![alice_proj],
        "scope=mine must return only Alice's owned project"
    );
}

#[tokio::test]
async fn list_projects_scope_team_returns_only_team_shared_projects() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_project_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();

    let team = upsert_team(&pool, "grp-x", "Team X").await.unwrap();
    sqlx::query(
        "INSERT INTO team_memberships (principal_key, team_id, source) \
         VALUES (?, ?, 'token_claim')",
    )
    .bind(alice.principal_key())
    .bind(&team.id)
    .execute(&pool)
    .await
    .unwrap();

    // Bob owns bob-shared, shared to team-x (Alice's team)
    let store = ProjectStore::new(Arc::new(spacebot::db::DbPool::Sqlite(pool.clone())));
    let bob_shared = store
        .create_project(CreateProjectInput {
            name: "bob-shared".to_string(),
            description: String::new(),
            icon: String::new(),
            tags: vec![],
            root_path: "/tmp/bob-shared".to_string(),
            settings: serde_json::json!({}),
        })
        .await
        .unwrap();
    set_ownership(
        &pool,
        "project",
        &bob_shared.id,
        None,
        &bob.principal_key(),
        Visibility::Team,
        Some(&team.id),
    )
    .await
    .unwrap();
    // Alice owns her own personal project — must NOT appear in team scope
    let _alice_own = seed_owned_project(&pool, &alice, "alice-own", "/tmp/alice-own").await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let ids = fetch_projects(app, &token, Some("team")).await;
    assert_eq!(
        ids,
        vec![bob_shared.id],
        "scope=team returns only team-shared; excludes own-personal"
    );
}

#[tokio::test]
async fn list_projects_scope_org_matches_unfiltered_for_non_admin() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_project_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    let alice_proj = seed_owned_project(&pool, &alice, "alice-proj", "/tmp/alice-p").await;
    let bob_proj = seed_owned_project(&pool, &bob, "bob-proj", "/tmp/bob-p").await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let mut ids = fetch_projects(app, &token, Some("org")).await;
    ids.sort();
    let mut expected = vec![alice_proj, bob_proj];
    expected.sort();
    assert_eq!(
        ids, expected,
        "scope=org returns all projects (admin-equivalent view for non-admin callers)"
    );
}

#[tokio::test]
async fn list_projects_no_scope_param_preserves_legacy_unfiltered_behavior() {
    // Absent ?scope= returns the full list. Regression guard against a
    // refactor that accidentally applies a default scope. Existing SPA
    // callers (pre-PR-5) depend on this.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_project_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    let alice_proj = seed_owned_project(&pool, &alice, "alice-proj", "/tmp/alice-p").await;
    let bob_proj = seed_owned_project(&pool, &bob, "bob-proj", "/tmp/bob-p").await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let mut ids = fetch_projects(app, &token, None).await;
    ids.sort();
    let mut expected = vec![alice_proj, bob_proj];
    expected.sort();
    assert_eq!(ids, expected, "no ?scope= param returns unfiltered list");
}

#[tokio::test]
async fn list_projects_scope_admin_bypass_returns_unfiltered_even_with_scope_mine() {
    // Admin's scope=mine returns the full list: admin bypasses the
    // ownership filter, same as the per-row authz admin bypass.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_project_store(&state, &pool);
    let admin = user_ctx("admin-carol", vec![ROLE_ADMIN]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &admin).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    let bob_proj = seed_owned_project(&pool, &bob, "bob-proj", "/tmp/bob-p").await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&admin);
    let ids = fetch_projects(app, &token, Some("mine")).await;
    assert_eq!(
        ids,
        vec![bob_proj],
        "admin + scope=mine sees every project (bypass ownership)"
    );
}

#[tokio::test]
async fn list_projects_scope_mine_returns_500_when_scope_query_fails() {
    // Regression guard for PR #115 review finding: scope-filter query
    // failures must surface as 500, not degrade to 200-empty. A silent
    // empty response leaves the user staring at a missing agent list
    // with no indication anything broke.
    //
    // Force the failure by closing the instance pool after state is
    // built but before the request reaches the handler. The scope-filter
    // helper will see a closed pool and return Err.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    attach_project_store(&state, &pool);
    let alice = user_ctx("alice", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();

    // Close the instance pool so the scope-filter sqlx query fails.
    // The pool reference on ApiState is an Arc<Option<SqlitePool>>; the
    // clone we hold here and the clone ApiState holds point at the
    // same underlying connections. Closing one is visible to both.
    pool.close().await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/agents/projects?scope=mine")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "scope-filter query failure must surface as 500, not silent-empty 200"
    );
}
