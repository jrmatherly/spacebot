//! Phase 3 integration tests: Graph group resolution + photo persistence.
//! Drives `sync_groups_for_principal` and `sync_user_photo_for_principal`
//! directly against a Wiremock-backed Graph stub. Skips the HTTP layer.

#[path = "support/mock_entra.rs"]
mod mock_entra;

use mock_entra::{
    MockTenant, mount_graph_stub, mount_obo_stub, mount_photo_stub, obo_endpoint_url,
};
use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::graph::{GraphClient, GraphConfig};
use sqlx::sqlite::SqlitePoolOptions;

use std::sync::Arc;

async fn setup_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("connect memory sqlite");
    sqlx::migrate!("./migrations/global")
        .run(&pool)
        .await
        .expect("run global migrations");
    pool
}

fn make_ctx(tid: &str, oid: &str, groups: Vec<&str>, overage: bool) -> AuthContext {
    AuthContext {
        principal_type: PrincipalType::User,
        tid: Arc::from(tid),
        oid: Arc::from(oid),
        roles: vec![],
        groups: groups.into_iter().map(Arc::from).collect(),
        groups_overage: overage,
        display_email: Some(Arc::from(format!("{oid}@example.com").as_str())),
        display_name: Some(Arc::from(format!("User {oid}").as_str())),
    }
}

fn graph_cfg(tenant_id: &str, server_uri: &str, obo_url: &str) -> GraphConfig {
    GraphConfig {
        tenant_id: Arc::from(tenant_id),
        web_api_client_id: Arc::from("test-client"),
        web_api_client_secret: Arc::from("test-secret"),
        graph_api_base: Arc::from(server_uri),
        obo_token_endpoint: Arc::from(obo_url),
        request_timeout_secs: 5,
    }
}

/// Overage path: token has `groups_overage = true`, helper calls Graph
/// `/me/getMemberObjects`, persists both groups into `team_memberships`.
#[tokio::test]
async fn resolves_transitive_groups_on_overage() {
    let tenant = MockTenant::start().await;
    mount_obo_stub(&tenant.server).await;
    mount_graph_stub(
        &tenant.server,
        vec![
            ("grp-111".into(), "Platform".into()),
            ("grp-222".into(), "Security".into()),
        ],
    )
    .await;

    let pool = setup_pool().await;
    let ctx = make_ctx(&tenant.tenant_id, "oid-overage", vec![], true);
    spacebot::auth::repository::upsert_user_from_auth(&pool, &ctx)
        .await
        .expect("upsert user");

    let graph = GraphClient::new(graph_cfg(
        &tenant.tenant_id,
        tenant.server.uri().as_str(),
        obo_endpoint_url(&tenant.server).as_str(),
    ))
    .expect("graph client");

    spacebot::auth::middleware::sync_groups_for_principal(
        &pool,
        &graph,
        &ctx,
        "fake-user-token",
        300,
    )
    .await
    .expect("sync_groups");

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM team_memberships WHERE principal_key = ?",
    )
    .bind(ctx.principal_key())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 2, "both stub groups should be persisted");
}

/// Fail-closed path: Graph base points at a guaranteed-dead port. The sync
/// helper returns an error and `team_memberships` stays empty. Phase 4's
/// authz layer will then return 403 for team-scoped resources, which is
/// the correct behaviour when membership cannot be determined.
#[tokio::test]
async fn fail_closed_when_graph_unreachable() {
    let pool = setup_pool().await;
    let ctx = make_ctx(
        "00000000-0000-0000-0000-000000000001",
        "oid-dead",
        vec![],
        true,
    );
    spacebot::auth::repository::upsert_user_from_auth(&pool, &ctx)
        .await
        .expect("upsert user");

    let graph = GraphClient::new(graph_cfg(
        "00000000-0000-0000-0000-000000000001",
        "http://127.0.0.1:1",
        "http://127.0.0.1:1/oauth2/v2.0/token",
    ))
    .expect("graph client");

    let result = spacebot::auth::middleware::sync_groups_for_principal(
        &pool,
        &graph,
        &ctx,
        "fake-token",
        300,
    )
    .await;
    assert!(result.is_err(), "sync should propagate error when Graph is dead");

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM team_memberships WHERE principal_key = ?",
    )
    .bind(ctx.principal_key())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0, "fail-closed: memberships must remain empty");
}

/// Cache TTL skip: when the oldest persisted membership is younger than
/// `ttl_secs`, the helper short-circuits before any Graph call. We point
/// Graph at a dead port to prove no HTTP request happens.
#[tokio::test]
async fn respects_cache_ttl_and_skips_graph() {
    let pool = setup_pool().await;
    let ctx = make_ctx("tid-1", "oid-cached", vec!["grp-cached"], false);
    spacebot::auth::repository::upsert_user_from_auth(&pool, &ctx)
        .await
        .expect("upsert user");
    spacebot::auth::repository::upsert_team(&pool, "grp-cached", "Cached")
        .await
        .expect("upsert team");

    sqlx::query(
        r#"
        INSERT INTO team_memberships (principal_key, team_id, observed_at, source)
        VALUES (?, 'team-grp-cached', strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-1 seconds'), 'token_claim')
        "#,
    )
    .bind(ctx.principal_key())
    .execute(&pool)
    .await
    .expect("seed membership");

    let graph = GraphClient::new(graph_cfg(
        "tid-1",
        "http://127.0.0.1:1",
        "http://127.0.0.1:1/oauth2/v2.0/token",
    ))
    .expect("graph client");

    let result = spacebot::auth::middleware::sync_groups_for_principal(
        &pool, &graph, &ctx, "fake-token", 300,
    )
    .await;
    assert!(
        result.is_ok(),
        "cache-fresh path should short-circuit, got {result:?}",
    );
}

/// A-19: photo bytes from Graph are base64-encoded and persisted, with
/// `photo_updated_at` stamped to anchor the weekly TTL.
#[tokio::test]
async fn syncs_user_photo_on_first_fetch() {
    let tenant = MockTenant::start().await;
    mount_obo_stub(&tenant.server).await;
    let photo_bytes: Vec<u8> = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F'];
    mount_photo_stub(&tenant.server, Some(photo_bytes.clone())).await;

    let pool = setup_pool().await;
    let ctx = make_ctx(&tenant.tenant_id, "oid-with-photo", vec![], false);
    spacebot::auth::repository::upsert_user_from_auth(&pool, &ctx)
        .await
        .expect("upsert user");

    let graph = GraphClient::new(graph_cfg(
        &tenant.tenant_id,
        tenant.server.uri().as_str(),
        obo_endpoint_url(&tenant.server).as_str(),
    ))
    .expect("graph client");

    spacebot::auth::middleware::sync_user_photo_for_principal(
        &pool,
        &graph,
        &ctx,
        "fake-token",
    )
    .await
    .expect("photo sync");

    let (b64, ts): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT display_photo_b64, photo_updated_at FROM users WHERE principal_key = ?",
    )
    .bind(ctx.principal_key())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(b64.is_some(), "display_photo_b64 should be populated");
    assert!(ts.is_some(), "photo_updated_at should be populated");
}

/// A-19: when Graph returns 404, persist `display_photo_b64 = NULL` but
/// stamp `photo_updated_at = now` so the TTL skip protects us from
/// re-fetching on the next request.
#[tokio::test]
async fn records_absent_photo_with_timestamp() {
    let tenant = MockTenant::start().await;
    mount_obo_stub(&tenant.server).await;
    mount_photo_stub(&tenant.server, None).await;

    let pool = setup_pool().await;
    let ctx = make_ctx(&tenant.tenant_id, "oid-no-photo", vec![], false);
    spacebot::auth::repository::upsert_user_from_auth(&pool, &ctx)
        .await
        .expect("upsert user");

    let graph = GraphClient::new(graph_cfg(
        &tenant.tenant_id,
        tenant.server.uri().as_str(),
        obo_endpoint_url(&tenant.server).as_str(),
    ))
    .expect("graph client");

    spacebot::auth::middleware::sync_user_photo_for_principal(
        &pool,
        &graph,
        &ctx,
        "fake-token",
    )
    .await
    .expect("photo sync");

    let (b64, ts): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT display_photo_b64, photo_updated_at FROM users WHERE principal_key = ?",
    )
    .bind(ctx.principal_key())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(b64.is_none(), "display_photo_b64 should be NULL on 404");
    assert!(
        ts.is_some(),
        "photo_updated_at should still stamp the TTL anchor",
    );
}
