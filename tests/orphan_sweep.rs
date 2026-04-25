//! Phase 10 Task 10.5: orphan-resource sweep regression coverage.
//! Pins the contract of `spacebot::admin::sweep_orphans` for both
//! directions (MissingOwnership + StaleOwnership) plus the negative
//! "partially-initialized agent" path required by IMPORTANT-7 #6.

use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::principals::Visibility;
use spacebot::auth::repository::{set_ownership, upsert_user_from_auth};
use sqlx::sqlite::SqlitePoolOptions;
use std::path::PathBuf;
use std::sync::Arc;

async fn instance_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations/global")
        .run(&pool)
        .await
        .unwrap();
    pool
}

fn alice_ctx() -> AuthContext {
    AuthContext {
        principal_type: PrincipalType::User,
        tid: Arc::from("t1"),
        oid: Arc::from("alice"),
        roles: vec![],
        groups: vec![],
        groups_overage: false,
        display_email: None,
        display_name: None,
    }
}

/// Set up an agent DB at `<tmp>/agents/<agent_id>/data/spacebot.db`
/// containing one `memories` row per supplied id. Returns the absolute
/// path to the created DB file.
async fn make_agent_db(tmp: &std::path::Path, agent_id: &str, memory_ids: &[&str]) -> PathBuf {
    let dir = tmp.join("agents").join(agent_id).join("data");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("spacebot.db");
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}?mode=rwc", path.display()))
        .await
        .unwrap();
    sqlx::query("CREATE TABLE memories (id TEXT PRIMARY KEY)")
        .execute(&pool)
        .await
        .unwrap();
    for id in memory_ids {
        sqlx::query("INSERT INTO memories (id) VALUES (?)")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }
    pool.close().await;
    path
}

#[tokio::test]
async fn sweep_reports_resources_without_ownership_rows() {
    let pool = instance_pool().await;
    let alice = alice_ctx();
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    set_ownership(
        &pool,
        "memory",
        "m-owned",
        None,
        &alice.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();

    // Agent DB has both m-owned (instance has the ownership row) and
    // m-orphan-1 (instance does NOT). The sweep must flag m-orphan-1
    // and leave m-owned alone.
    let tmp = tempfile::tempdir().expect("tempdir");
    let agent_db_path = make_agent_db(tmp.path(), "agent-test", &["m-owned", "m-orphan-1"]).await;

    let orphans = spacebot::admin::sweep_orphans(&pool, &[agent_db_path])
        .await
        .unwrap();

    assert!(
        orphans.iter().any(|o| o.resource_id == "m-orphan-1"
            && matches!(o.kind, spacebot::admin::OrphanKind::MissingOwnership)),
        "expected m-orphan-1 in MissingOwnership, got {orphans:?}",
    );
    assert!(
        !orphans.iter().any(|o| o.resource_id == "m-owned"),
        "m-owned has an ownership row, must NOT be in orphan list, got {orphans:?}",
    );
}

#[tokio::test]
async fn sweep_tolerates_partially_initialized_agent_directory() {
    // IMPORTANT-7 #6: an agent directory exists but `spacebot.db` is
    // missing (partial init or concurrent delete). Sweep must not crash;
    // it must log and proceed.
    let pool = instance_pool().await;
    let tmp = tempfile::tempdir().expect("tempdir");
    let stub_dir = tmp.path().join("agents").join("agent-half").join("data");
    std::fs::create_dir_all(&stub_dir).unwrap();
    let nonexistent_db = stub_dir.join("spacebot.db");
    assert!(
        !nonexistent_db.exists(),
        "test invariant: db must be missing"
    );

    // A second, valid agent DB so we can confirm the sweep continues
    // past the broken one and still produces output.
    let alice = alice_ctx();
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    let valid_db = make_agent_db(tmp.path(), "agent-good", &["m-fresh"]).await;

    let orphans = spacebot::admin::sweep_orphans(&pool, &[nonexistent_db, valid_db])
        .await
        .expect("sweep must not error on partial-init agent");

    // m-fresh has no ownership row, so it should still be flagged.
    assert!(
        orphans.iter().any(|o| o.resource_id == "m-fresh"
            && matches!(o.kind, spacebot::admin::OrphanKind::MissingOwnership)),
        "expected sweep to continue past the broken agent and find m-fresh; got {orphans:?}",
    );
}

#[tokio::test]
async fn sweep_reports_stale_ownership_when_agent_db_is_gone() {
    // I4: Direction-2 of the sweep must flag rows in resource_ownership
    // whose owning agent has no per-agent DB on disk. Without this
    // coverage the StaleOwnership branch (src/admin.rs Direction-2)
    // would be unverified despite being half the sweep's contract.
    let pool = instance_pool().await;
    let alice = alice_ctx();
    upsert_user_from_auth(&pool, &alice).await.unwrap();

    let tmp = tempfile::tempdir().expect("tempdir");
    // SPACEBOT_DIR is the resolution root for Direction-2's reverse
    // path lookup. Setting it makes the sweep look under
    // `<tmp>/agents/<agent_id>/data/spacebot.db`.
    // SAFETY: nextest's process-per-test isolation prevents this from
    // racing the env-unset test below.
    unsafe {
        std::env::set_var("SPACEBOT_DIR", tmp.path());
    }

    // Insert an ownership row whose owner_agent_id = "ghost-agent".
    // No `<tmp>/agents/ghost-agent/` directory exists, so the sweep's
    // Direction-2 path-existence check fires and produces a
    // StaleOwnership entry.
    sqlx::query(
        "INSERT INTO resource_ownership \
         (resource_type, resource_id, owner_agent_id, owner_principal_key, \
          visibility, shared_with_team_id) \
         VALUES (?, ?, ?, ?, 'personal', NULL)",
    )
    .bind("memory")
    .bind("m-stale")
    .bind("ghost-agent")
    .bind(alice.principal_key())
    .execute(&pool)
    .await
    .unwrap();

    // Direction-1 needs no agent DBs to scan; we're only exercising the
    // reverse-direction path resolution.
    let orphans = spacebot::admin::sweep_orphans(&pool, &[]).await.unwrap();

    assert!(
        orphans.iter().any(|o| o.resource_id == "m-stale"
            && matches!(o.kind, spacebot::admin::OrphanKind::StaleOwnership)),
        "expected m-stale in StaleOwnership; got {orphans:?}",
    );
}

#[tokio::test]
async fn sweep_rejects_path_traversal_in_owner_agent_id() {
    // C3 regression guard: a malicious owner_agent_id like
    // "../../etc" must not redirect the sweep to read arbitrary files.
    // is_safe_agent_id rejects the value, so the sweep skips the row
    // entirely and produces no orphans for it.
    let pool = instance_pool().await;
    let alice = alice_ctx();
    upsert_user_from_auth(&pool, &alice).await.unwrap();

    let tmp = tempfile::tempdir().expect("tempdir");
    unsafe {
        std::env::set_var("SPACEBOT_DIR", tmp.path());
    }

    sqlx::query(
        "INSERT INTO resource_ownership \
         (resource_type, resource_id, owner_agent_id, owner_principal_key, \
          visibility, shared_with_team_id) \
         VALUES (?, ?, ?, ?, 'personal', NULL)",
    )
    .bind("memory")
    .bind("m-evil")
    .bind("../../some-other-path")
    .bind(alice.principal_key())
    .execute(&pool)
    .await
    .unwrap();

    let orphans = spacebot::admin::sweep_orphans(&pool, &[]).await.unwrap();
    // The traversal-attempt row must NOT produce a StaleOwnership entry
    // (because we never opened the bogus path); it's silently dropped
    // with a tracing::warn!. The audit-log surface still records the
    // sweep run via the calling endpoint's AdminRead emission.
    assert!(
        !orphans.iter().any(|o| o.resource_id == "m-evil"),
        "path-traversal owner_agent_id must not appear in orphan output; got {orphans:?}",
    );
}

#[tokio::test]
async fn discover_agent_db_paths_returns_empty_when_env_unset() {
    // Pin: discover_agent_db_paths gracefully handles unset env var.
    // SAFETY: this single-threaded test is the only one in this file that
    // depends on the absence of SPACEBOT_DIR; nextest's process-per-test
    // isolation keeps it from racing the sweep tests.
    unsafe {
        std::env::remove_var("SPACEBOT_DIR");
    }
    assert!(
        spacebot::admin::discover_agent_db_paths().is_empty(),
        "no SPACEBOT_DIR ⇒ no paths discovered",
    );
}
