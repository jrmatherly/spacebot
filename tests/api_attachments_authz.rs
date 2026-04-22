//! Phase 4 PR 2 T4.13c — handler-level authz tests for `src/api/attachments.rs`.
//!
//! Mirrors `tests/api_tasks_authz.rs` and `tests/api_portal_authz.rs`:
//! exercises the full stack (Entra middleware with `MockValidator`, the
//! per-handler inline `check_read_with_audit` / `check_write` blocks, a
//! real `resource_ownership` row in the instance pool, and a real
//! `saved_attachments` row in the agent-scoped pool).
//!
//! Attachments key on their own ownership row (`resource_type =
//! "saved_attachment"`, A-09 bare UUID). Parent-resource relationships
//! (message_id, task, memory) are NOT consulted; the per-attachment
//! ownership entry is the source of truth. A non-owner GET on a
//! personal attachment must collapse to 404 (hide-existence), mirroring
//! the tasks/memories pattern.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use spacebot::api::ApiState;
use spacebot::api::test_support::build_test_router_entra;
use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::principals::Visibility;
use spacebot::auth::repository::{get_ownership, set_ownership, upsert_user_from_auth};
use spacebot::auth::roles::{ROLE_ADMIN, ROLE_USER};
use spacebot::auth::testing::mint_mock_token;
use sqlx::sqlite::SqlitePoolOptions;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
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

/// Attach a per-agent SQLite pool with the `saved_attachments` schema,
/// a temporary workspace directory, and a seeded `channels` row (the
/// `saved_attachments` table has a FK into `channels`) for the given
/// `agent_id` + `channel_id`. Returns the pool and a `TempDir` guard;
/// keep both alive for the duration of the test.
async fn attach_agent_pool_and_workspace(
    state: &ApiState,
    agent_id: &str,
    channel_id: &str,
) -> (sqlx::SqlitePool, TempDir) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("sqlite in-memory agent pool");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("per-agent migrations apply cleanly");
    sqlx::query("INSERT INTO channels (id, platform) VALUES (?, 'portal')")
        .bind(channel_id)
        .execute(&pool)
        .await
        .expect("seed channels row (FK parent for saved_attachments)");
    let mut pools = HashMap::new();
    pools.insert(agent_id.to_string(), pool.clone());
    state.set_agent_pools(pools);

    let tmp = tempfile::tempdir().expect("tempdir");
    let ws_path: PathBuf = tmp.path().to_path_buf();
    tokio::fs::create_dir_all(ws_path.join("saved"))
        .await
        .expect("create saved dir");
    let mut workspaces = HashMap::new();
    workspaces.insert(agent_id.to_string(), ws_path);
    state.set_agent_workspaces(workspaces);

    (pool, tmp)
}

/// Seed a `saved_attachments` row in the agent pool and a matching
/// `resource_ownership` row in the instance pool. Returns the
/// attachment UUID.
async fn seed_attachment(
    agent_pool: &sqlx::SqlitePool,
    instance_pool: &sqlx::SqlitePool,
    channel_id: &str,
    owner: &AuthContext,
    saved_dir: &std::path::Path,
) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let saved_filename = format!("{id}.bin");
    let disk_path = saved_dir.join(&saved_filename);
    tokio::fs::write(&disk_path, b"hello world")
        .await
        .expect("write fixture bytes");

    sqlx::query(
        "INSERT INTO saved_attachments \
         (id, channel_id, original_filename, saved_filename, mime_type, size_bytes, disk_path) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(channel_id)
    .bind("file.bin")
    .bind(&saved_filename)
    .bind("application/octet-stream")
    .bind(11_i64)
    .bind(disk_path.to_string_lossy().to_string())
    .execute(agent_pool)
    .await
    .expect("insert saved_attachment");

    set_ownership(
        instance_pool,
        "saved_attachment",
        &id,
        None,
        &owner.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .expect("set_ownership attachment");
    id
}

fn req_get_attachment(agent_id: &str, attachment_id: &str, bearer: &str) -> Request<Body> {
    Request::builder()
        .uri(format!(
            "/api/agents/{agent_id}/attachments/{attachment_id}"
        ))
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .body(Body::empty())
        .unwrap()
}

/// Build a minimal multipart/form-data body with a single file part
/// named `file` and the supplied bytes. Matches axum's multipart parser
/// so `field.chunk()` yields the contents back on the handler side.
fn multipart_body(boundary: &str, filename: &str, bytes: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n\
             Content-Type: application/octet-stream\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

fn req_upload_attachment(
    agent_id: &str,
    channel_id: &str,
    bearer: &str,
    filename: &str,
    bytes: &[u8],
) -> Request<Body> {
    let boundary = "---------------------------spacebotboundary";
    let body = multipart_body(boundary, filename, bytes);
    Request::builder()
        .method("POST")
        .uri(format!(
            "/api/agents/{agent_id}/channels/{channel_id}/attachments/upload"
        ))
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap()
}

#[tokio::test]
async fn non_owner_get_attachment_returns_404() {
    // Bob reading Alice's personal attachment must see 404 (hide
    // existence), not 403. Guards the read gate on serve_attachment and
    // documents that parent-resource relationships don't broaden access:
    // only the per-attachment ownership row matters.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let (agent_pool, _tmp) = attach_agent_pool_and_workspace(&state, "agent-a", "chan-1").await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let bob = user_ctx("bob", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();

    let saved_dir = state
        .agent_workspaces
        .load()
        .get("agent-a")
        .expect("workspace attached")
        .join("saved");
    let attachment_id =
        seed_attachment(&agent_pool, &pool, "chan-1", &alice, &saved_dir).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let res = app
        .oneshot(req_get_attachment("agent-a", &attachment_id, &token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "non-owner on alice's personal attachment must see 404 (hide existence), not 403"
    );
}

#[tokio::test]
async fn owner_get_attachment_returns_200() {
    // Positive-path guard: the owner passes the check_read_with_audit
    // gate, the handler reads the row from the agent pool and the bytes
    // from the temp workspace, and returns 200.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let (agent_pool, _tmp) = attach_agent_pool_and_workspace(&state, "agent-a", "chan-1").await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();

    let saved_dir = state
        .agent_workspaces
        .load()
        .get("agent-a")
        .expect("workspace attached")
        .join("saved");
    let attachment_id =
        seed_attachment(&agent_pool, &pool, "chan-1", &alice, &saved_dir).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&alice);
    let res = app
        .oneshot(req_get_attachment("agent-a", &attachment_id, &token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::OK,
        "owner must see 200 on their own attachment (got {:?})",
        res.status()
    );
}

#[tokio::test]
async fn admin_bypass_attachment_read() {
    // Admin bypass: a SpacebotAdmin role skips per-resource ownership.
    // Regression guard against `is_admin` returning false on the
    // serve_attachment gate.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let (agent_pool, _tmp) = attach_agent_pool_and_workspace(&state, "agent-a", "chan-1").await;
    let alice = user_ctx("alice", vec![ROLE_USER]);
    let admin = user_ctx("admin-carol", vec![ROLE_ADMIN]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &admin).await.unwrap();

    let saved_dir = state
        .agent_workspaces
        .load()
        .get("agent-a")
        .expect("workspace attached")
        .join("saved");
    let attachment_id =
        seed_attachment(&agent_pool, &pool, "chan-1", &alice, &saved_dir).await;

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&admin);
    let res = app
        .oneshot(req_get_attachment("agent-a", &attachment_id, &token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::OK,
        "admin must bypass per-resource ownership on GET /agents/{{id}}/attachments/{{id}}"
    );
}

#[tokio::test]
async fn create_attachment_assigns_ownership() {
    // A-12: the upload handler MUST `.await` set_ownership after the
    // insert. A tokio::spawn fire-and-forget would leave a window where
    // the uploader's immediate GET races into a NotOwned 404. The proof
    // is an ownership row present synchronously after the POST completes.
    //
    // The upload's pre-check rides the agent ownership row, so seed
    // alice as the owner of agent-a first.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let (_agent_pool, _tmp) = attach_agent_pool_and_workspace(&state, "agent-a", "chan-1").await;
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
        .oneshot(req_upload_attachment(
            "agent-a",
            "chan-1",
            &token,
            "hello.txt",
            b"hello",
        ))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::OK,
        "upload_attachment must succeed for the agent owner (got {:?})",
        res.status()
    );
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let attachment_id = parsed["id"]
        .as_str()
        .expect("attachment id in response")
        .to_string();

    let own = get_ownership(&pool, "saved_attachment", &attachment_id)
        .await
        .unwrap()
        .expect("ownership row must be present synchronously after POST");
    assert_eq!(
        own.owner_principal_key,
        alice.principal_key(),
        "owner principal_key must be the uploader (alice)"
    );
    assert_eq!(own.visibility, "personal");
}

#[tokio::test]
async fn pool_none_skip_get_attachment() {
    // Regression guard for the early-startup / static-token fallback path.
    // When instance_pool is not attached, the serve_attachment gate skips
    // and the handler proceeds to the agent-pool lookup (which returns
    // 404 since no agent pool is attached either). Assertion: NOT 401/403,
    // proving the request passed auth + the no-op authz skip.
    let state = ApiState::new_test_state_with_mock_entra_no_pool();
    let bob = user_ctx("bob", vec![ROLE_USER]);

    let app = build_test_router_entra(state);
    let token = mint_mock_token(&bob);
    let res = app
        .oneshot(req_get_attachment("agent-a", "any-id", &token))
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
