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
    state.set_project_store(Arc::new(ProjectStore::new(pool.clone())));
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
    let store = ProjectStore::new(pool.clone());
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
        let store = ProjectStore::new(pool.clone());
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
