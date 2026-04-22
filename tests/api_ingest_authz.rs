//! Phase 4 PR 2 T4.13d — handler-level authz tests for `src/api/ingest.rs`.
//!
//! Mirrors `tests/api_tasks_authz.rs` and `tests/api_portal_authz.rs`:
//! exercises the full stack (Entra middleware with MockValidator,
//! handler `check_read_with_audit` / `check_write`, policy module
//! against a real `resource_ownership` row). Ingest has three handlers
//! spanning two authz shapes: agent-scoped read (`list_ingest_files`),
//! agent-scoped write with per-file ownership registration
//! (`upload_ingest_file`), and file-scoped write keyed on content_hash
//! (`delete_ingest_file`). Five tests cover the three gate shapes:
//! non-owner read, owner read, admin bypass, create-assigns-ownership,
//! pool-None skip. Policy module is exercised separately through its
//! own 15+ tests in `tests/policy_table.rs`.
//!
//! Uploads use a hand-rolled multipart body with a fixed boundary —
//! axum's Multipart extractor parses any RFC-7578 compliant stream.
//! The content_hash the handler computes is deterministic
//! (`Sha256::digest(bytes)`), so the test reads the ownership row back
//! by the same hash it knows the handler will emit.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use sqlx::sqlite::SqlitePoolOptions;
use spacebot::api::ApiState;
use spacebot::api::test_support::build_test_router_entra;
use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::principals::Visibility;
use spacebot::auth::repository::{get_ownership, set_ownership, upsert_user_from_auth};
use spacebot::auth::roles::{ROLE_ADMIN, ROLE_USER};
use spacebot::auth::testing::mint_mock_token;
use std::collections::HashMap;
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

/// Seed an in-memory per-agent pool with the ingestion_files schema and
/// wire it into `state.agent_pools` under `agent_id`. Required because
/// `list_ingest_files` and `delete_ingest_file` dereference
/// `state.agent_pools[agent_id]` after the gate. Also seeds
/// `state.agent_workspaces` with a tempdir so `upload_ingest_file`
/// passes the workspace lookup that follows the write gate.
async fn attach_agent_pool_and_workspace(
    state: &ApiState,
    agent_id: &str,
) -> (sqlx::SqlitePool, tempfile::TempDir) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("sqlite in-memory agent pool");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("per-agent migrations apply cleanly");
    let mut pools = HashMap::new();
    pools.insert(agent_id.to_string(), pool.clone());
    state.set_agent_pools(pools);

    let workspace = tempfile::tempdir().expect("tempdir for agent workspace");
    let mut workspaces = HashMap::new();
    workspaces.insert(agent_id.to_string(), workspace.path().to_path_buf());
    state.set_agent_workspaces(workspaces);

    (pool, workspace)
}

fn req_list_ingest_files(agent_id: &str, bearer: &str) -> Request<Body> {
    Request::builder()
        .uri(format!("/api/agents/ingest/files?agent_id={agent_id}"))
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap()
}

/// Build a minimal multipart/form-data POST body for the upload handler.
/// Boundary is fixed so the body is byte-predictable; any value is fine
/// as long as it doesn't appear in the payload.
fn req_upload_ingest_file(agent_id: &str, bearer: &str, filename: &str, bytes: &[u8]) -> Request<Body> {
    let boundary = "----test-boundary-spacebot-ingest-authz";
    let mut body: Vec<u8> = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: text/plain\r\n\r\n");
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    Request::builder()
        .method("POST")
        .uri(format!("/api/agents/ingest/files?agent_id={agent_id}"))
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap()
}

#[tokio::test]
async fn non_owner_get_ingest_file_returns_404() {
    // Bob lists ingest files for Alice's personal agent. The agent-read
    // gate fires before the per-agent pool is reached, so Bob sees 404
    // (hide existence) per DenyReason::NotYours.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let _fixture = attach_agent_pool_and_workspace(&state, "agent-a").await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    set_ownership(
        &pool,
        "agent",
        "agent-a",
        None,
        &alice.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let res = app
        .oneshot(req_list_ingest_files("agent-a", &token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "non-owner on alice's personal agent must see 404 (hide existence), not 403"
    );
}

#[tokio::test]
async fn owner_get_ingest_file_returns_200() {
    // Alice lists files on her own agent. Agent pool and workspace are
    // attached so the handler reaches the happy path after passing the
    // gate. An empty ingestion_files table returns an empty list.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let _fixture = attach_agent_pool_and_workspace(&state, "agent-a").await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    set_ownership(
        &pool,
        "agent",
        "agent-a",
        None,
        &alice.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let res = app
        .oneshot(req_list_ingest_files("agent-a", &token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::OK,
        "owner must see 200 on their own agent's ingest files (got {:?})",
        res.status()
    );
}

#[tokio::test]
async fn admin_bypass_ingest_read() {
    // Admin bypass: a SpacebotAdmin role skips per-resource ownership.
    // Regression guard against `is_admin` returning false on the ingest
    // handler gate. The downstream 200 path requires agent_pools +
    // workspaces so the handler can reach the store after the gate.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let _fixture = attach_agent_pool_and_workspace(&state, "agent-a").await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let admin = user_ctx("admin-carol", vec![ROLE_ADMIN]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &admin).await.unwrap();
    set_ownership(
        &pool,
        "agent",
        "agent-a",
        None,
        &alice.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&admin);
    let res = app
        .oneshot(req_list_ingest_files("agent-a", &token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::OK,
        "admin must bypass per-resource ownership on GET /agents/ingest/files"
    );
}

#[tokio::test]
async fn create_ingest_file_assigns_ownership() {
    // A-12: the POST /agents/ingest/files handler MUST `.await`
    // set_ownership("ingestion_file", content_hash, ...) before returning.
    // A tokio::spawn fire-and-forget would leave a window where the
    // uploader's immediate DELETE races into a NotOwned 404. The proof is
    // an ownership row keyed on the deterministic content_hash present
    // synchronously after the POST completes.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let _fixture = attach_agent_pool_and_workspace(&state, "agent-a").await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    // Upload writes through the agent-write gate; Alice must own the
    // agent for the upload to succeed.
    set_ownership(
        &pool,
        "agent",
        "agent-a",
        None,
        &alice.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();

    let payload = b"hello spacebot ingest authz";
    let expected_hash = spacebot::agent::ingestion::content_hash(
        std::str::from_utf8(payload).unwrap(),
    );

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let res = app
        .oneshot(req_upload_ingest_file(
            "agent-a",
            &token,
            "doc.txt",
            payload,
        ))
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "upload_ingest_file must succeed for the agent owner (got {:?})",
        res.status()
    );

    let own = get_ownership(&pool, "ingestion_file", &expected_hash)
        .await
        .unwrap()
        .expect("ownership row must be present synchronously after POST");
    assert_eq!(
        own.owner_principal_key,
        alice.principal_key(),
        "owner principal_key must be the uploader (alice)"
    );
    assert_eq!(own.visibility, "personal");
    assert_eq!(
        own.owner_agent_id.as_deref(),
        Some("agent-a"),
        "owner_agent_id must carry the agent scope for per-agent queries"
    );
}

#[tokio::test]
async fn pool_none_skip_get_ingest_file() {
    // Regression guard for the early-startup / static-token fallback path.
    // When instance_pool is not attached, the list_ingest_files gate skips
    // and the handler proceeds to the agent_pools lookup (404 because no
    // pool is registered). Assertion: NOT 401/403, proving the request
    // passed auth + the no-op authz skip.
    let state = ApiState::new_test_state_with_mock_entra_no_pool();
    let bob = user_ctx("bob", vec![ROLE_USER]);

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let res = app
        .oneshot(req_list_ingest_files("agent-a", &token))
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
