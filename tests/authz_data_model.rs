//! Integration test: Phase 2 migrations + repository helpers against a
//! real SQLite instance. Uses the crate's global-migration path.

use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::principals::{ResourceScope, Visibility};
use spacebot::auth::repository::{
    RepositoryError, get_ownership, get_teams_by_ids, list_ownerships_by_ids,
    list_resource_ids_owned_by, set_ownership, upsert_team, upsert_user_from_auth,
};
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;

async fn setup_pool() -> sqlx::SqlitePool {
    // :memory: in-process for each test. Migrations run against it.
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("connect memory sqlite");

    // Migrations directory (instance-level):
    sqlx::migrate!("./migrations/global")
        .run(&pool)
        .await
        .expect("run global migrations");
    pool
}

fn make_ctx(tid: &str, oid: &str) -> AuthContext {
    AuthContext {
        principal_type: PrincipalType::User,
        tid: Arc::from(tid),
        oid: Arc::from(oid),
        roles: vec![],
        groups: vec![],
        groups_overage: false,
        display_email: Some(Arc::from(format!("{oid}@example.com").as_str())),
        display_name: Some(Arc::from(format!("User {oid}").as_str())),
    }
}

#[tokio::test]
async fn migrations_apply_cleanly() {
    let _pool = setup_pool().await;
    // If we got here, all global migrations applied without error.
}

#[tokio::test]
async fn upsert_user_is_idempotent() {
    let pool = setup_pool().await;
    let ctx = make_ctx("tid-1", "oid-a");
    let first = upsert_user_from_auth(&pool, &ctx)
        .await
        .expect("first upsert");
    let second = upsert_user_from_auth(&pool, &ctx)
        .await
        .expect("second upsert");
    assert_eq!(first.principal_key, second.principal_key);
    assert_eq!(
        first.created_at, second.created_at,
        "created_at must be stable"
    );
    assert!(
        second.updated_at >= first.updated_at,
        "updated_at should not regress"
    );
}

#[tokio::test]
async fn team_upsert_keys_on_external_id() {
    let pool = setup_pool().await;
    let t1 = upsert_team(&pool, "grp-111", "Platform").await.expect("t1");
    let t2 = upsert_team(&pool, "grp-111", "Platform (renamed)")
        .await
        .expect("t2");
    assert_eq!(t1.id, t2.id);
    assert_eq!(t2.display_name, "Platform (renamed)");
}

#[tokio::test]
async fn ownership_write_then_read_roundtrips() {
    let pool = setup_pool().await;
    let ctx = make_ctx("tid-1", "oid-a");
    upsert_user_from_auth(&pool, &ctx).await.unwrap();
    let key = ctx.principal_key();

    set_ownership(
        &pool,
        "memory",
        "mem-42",
        Some("agent-alpha"),
        &key,
        Visibility::Personal,
        None,
    )
    .await
    .expect("set ownership");

    let got = get_ownership(&pool, "memory", "mem-42")
        .await
        .expect("read ownership")
        .expect("row present");
    assert_eq!(got.owner_principal_key, key);
    assert_eq!(got.visibility, "personal");
    assert_eq!(got.owner_agent_id, Some("agent-alpha".to_string()));
}

#[tokio::test]
async fn team_visibility_requires_team_id() {
    let pool = setup_pool().await;
    let ctx = make_ctx("tid-1", "oid-a");
    upsert_user_from_auth(&pool, &ctx).await.unwrap();
    let key = ctx.principal_key();

    let result = set_ownership(
        &pool,
        "memory",
        "mem-42",
        None,
        &key,
        Visibility::Team,
        None, // missing team_id!
    )
    .await;
    assert!(
        result.is_err(),
        "CHECK constraint must reject team visibility without team_id"
    );
}

#[tokio::test]
async fn upsert_user_rejects_legacy_static_principal() {
    let pool = setup_pool().await;
    let ctx = AuthContext::legacy_static();
    let err = upsert_user_from_auth(&pool, &ctx)
        .await
        .expect_err("legacy_static principals must be rejected");
    assert!(
        matches!(err, RepositoryError::InvalidPrincipalType),
        "expected InvalidPrincipalType, got {err:?}",
    );
}

#[tokio::test]
async fn get_ownership_returns_none_for_missing_row() {
    let pool = setup_pool().await;
    let missing = get_ownership(&pool, "memory", "never-created")
        .await
        .expect("read ownership");
    assert!(
        missing.is_none(),
        "get_ownership must return None, not Err, for an unknown resource",
    );
}

#[tokio::test]
async fn raw_visibility_insert_rejects_unknown_value() {
    // Guards the CHECK (visibility IN ('personal', 'team', 'org')) constraint
    // against SQL paths that bypass the Visibility enum (migrations, admin
    // tooling, backfill scripts). Uses the legacy 'global' value from the
    // research draft, which is the most likely regression vector.
    let pool = setup_pool().await;
    let ctx = make_ctx("tid-1", "oid-a");
    upsert_user_from_auth(&pool, &ctx).await.unwrap();
    let key = ctx.principal_key();

    let result = sqlx::query(
        r#"
        INSERT INTO resource_ownership (
            resource_type, resource_id, owner_principal_key, visibility
        )
        VALUES (?, ?, ?, ?)
        "#,
    )
    .bind("memory")
    .bind("mem-global")
    .bind(&key)
    .bind("global")
    .execute(&pool)
    .await;
    assert!(
        result.is_err(),
        "CHECK constraint must reject visibility = 'global'"
    );
}

// ---------------------------------------------------------------------------
// CHECK-constraint regression tests for the 4 enum columns tightened in
// migrations 1-3 (users.principal_type, users.status, teams.status,
// team_memberships.source). Each test binds a raw sqlx::query with an
// invalid value so the CHECK is the sole failure source; matching record
// types use bare `String` in Rust, so these CHECKs are the only guard
// against direct-SQL paths (migrations, admin tooling, backfill scripts)
// emitting garbage enum values. Pairs with
// `raw_visibility_insert_rejects_unknown_value` above.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn raw_users_insert_rejects_unknown_principal_type() {
    let pool = setup_pool().await;
    let result = sqlx::query(
        r#"
        INSERT INTO users (principal_key, tenant_id, object_id, principal_type)
        VALUES (?, ?, ?, ?)
        "#,
    )
    .bind("tid-x:oid-x")
    .bind("tid-x")
    .bind("oid-x")
    .bind("guest") // not in ('user', 'service_principal', 'system')
    .execute(&pool)
    .await;
    assert!(
        result.is_err(),
        "CHECK constraint must reject principal_type = 'guest'"
    );
}

#[tokio::test]
async fn raw_users_insert_rejects_unknown_status() {
    let pool = setup_pool().await;
    let result = sqlx::query(
        r#"
        INSERT INTO users (principal_key, tenant_id, object_id, principal_type, status)
        VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind("tid-x:oid-x")
    .bind("tid-x")
    .bind("oid-x")
    .bind("user")
    .bind("suspended") // not in ('active', 'disabled', 'deleted')
    .execute(&pool)
    .await;
    assert!(
        result.is_err(),
        "CHECK constraint must reject users.status = 'suspended'"
    );
}

#[tokio::test]
async fn raw_teams_insert_rejects_unknown_status() {
    let pool = setup_pool().await;
    let result = sqlx::query(
        r#"
        INSERT INTO teams (id, external_id, display_name, status)
        VALUES (?, ?, ?, ?)
        "#,
    )
    .bind("team-xyz")
    .bind("grp-xyz")
    .bind("Engineering")
    .bind("retired") // not in ('active', 'archived')
    .execute(&pool)
    .await;
    assert!(
        result.is_err(),
        "CHECK constraint must reject teams.status = 'retired'"
    );
}

#[tokio::test]
async fn raw_team_memberships_insert_rejects_unknown_source() {
    let pool = setup_pool().await;
    let ctx = make_ctx("tid-1", "oid-a");
    upsert_user_from_auth(&pool, &ctx).await.unwrap();
    let team = upsert_team(&pool, "grp-777", "Team 777").await.unwrap();
    let key = ctx.principal_key();

    let result = sqlx::query(
        r#"
        INSERT INTO team_memberships (principal_key, team_id, source)
        VALUES (?, ?, ?)
        "#,
    )
    .bind(&key)
    .bind(&team.id)
    .bind("manual_admin") // not in ('token_claim', 'graph_overage')
    .execute(&pool)
    .await;
    assert!(
        result.is_err(),
        "CHECK constraint must reject team_memberships.source = 'manual_admin'"
    );
}

#[tokio::test]
async fn list_ownerships_by_ids_empty_short_circuits() {
    let pool = setup_pool().await;
    let result = list_ownerships_by_ids(&pool, "memory", &[]).await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn list_ownerships_by_ids_returns_map_keyed_by_resource_id() {
    let pool = setup_pool().await;
    let ctx = make_ctx("tid-1", "oid-a");
    upsert_user_from_auth(&pool, &ctx).await.unwrap();
    let key = ctx.principal_key();
    let team = upsert_team(&pool, "grp-batch", "Batch Team").await.unwrap();

    set_ownership(
        &pool,
        "memory",
        "m-1",
        Some("agent-1"),
        &key,
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();
    set_ownership(
        &pool,
        "memory",
        "m-2",
        Some("agent-1"),
        &key,
        Visibility::Team,
        Some(&team.id),
    )
    .await
    .unwrap();

    let ids = vec![
        "m-1".to_string(),
        "m-2".to_string(),
        "m-missing".to_string(),
    ];
    let map = list_ownerships_by_ids(&pool, "memory", &ids).await.unwrap();
    assert_eq!(map.len(), 2, "only 2 ownership rows exist");
    assert_eq!(
        map.get("m-1").unwrap().visibility.as_str(),
        Visibility::Personal.as_str()
    );
    let m2 = map.get("m-2").unwrap();
    assert_eq!(m2.visibility.as_str(), Visibility::Team.as_str());
    assert_eq!(m2.shared_with_team_id.as_deref(), Some(team.id.as_str()));
    assert!(
        !map.contains_key("m-missing"),
        "missing ids must not appear in the map so callers can detect no-row state"
    );
}

#[tokio::test]
async fn get_teams_by_ids_returns_display_names() {
    let pool = setup_pool().await;
    let t1 = upsert_team(&pool, "grp-alpha", "Alpha").await.unwrap();
    let t2 = upsert_team(&pool, "grp-beta", "Beta").await.unwrap();

    let ids = vec![t1.id.clone(), t2.id.clone(), "team-missing".to_string()];
    let map = get_teams_by_ids(&pool, &ids).await.unwrap();
    assert_eq!(map.len(), 2);
    assert_eq!(map.get(&t1.id).unwrap().display_name, "Alpha");
    assert_eq!(map.get(&t2.id).unwrap().display_name, "Beta");
    assert!(!map.contains_key("team-missing"));

    let empty = get_teams_by_ids(&pool, &[]).await.unwrap();
    assert!(empty.is_empty());
}

#[tokio::test]
async fn list_ownerships_by_ids_dedupes_duplicate_inputs() {
    // S2 (pr-test-analyzer): a caller flattening paginated results could
    // pass the same id twice. The SQL IN clause dedupes, and the HashMap
    // assembly overwrites the second row with identical data. Pin the
    // contract so a future regression (e.g., switching to Vec<Record> with
    // duplicate-preserving semantics) is caught.
    let pool = setup_pool().await;
    let ctx = make_ctx("tid-dup", "oid-dup");
    upsert_user_from_auth(&pool, &ctx).await.unwrap();
    let key = ctx.principal_key();

    set_ownership(
        &pool,
        "memory",
        "m-dup",
        Some("agent-dup"),
        &key,
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();

    let ids = vec![
        "m-dup".to_string(),
        "m-dup".to_string(),
        "m-dup".to_string(),
    ];
    let map = list_ownerships_by_ids(&pool, "memory", &ids).await.unwrap();
    assert_eq!(
        map.len(),
        1,
        "duplicate ids collapse to a single HashMap entry; the IN clause \
         returns one row per distinct key regardless of how many times it \
         appeared in the input"
    );
    assert!(map.contains_key("m-dup"));
}

#[tokio::test]
async fn list_ownerships_by_ids_handles_small_batches_up_to_known_safe_cap() {
    // I3 (pr-test-analyzer): SQLite's SQLITE_MAX_VARIABLE_NUMBER defaults
    // to 999 on older builds and 32766 on recent ones. One bind for
    // resource_type plus N for ids caps the safe input at 998 worst-case.
    // Phase 7 list handlers page results (<=200 today), but a future
    // caller flattening pagination could exceed that. This test pins the
    // safe region (N=500) so a future regression in placeholder emission
    // (e.g., accidental quadratic allocation, or an off-by-one in the
    // bind count) is caught without forcing a guard into the helper.
    //
    // The helper does not chunk; callers of list_ownerships_by_ids must
    // stay under SQLITE_MAX_VARIABLE_NUMBER. If/when a caller approaches
    // the cap, add chunk-loop logic here rather than relying on SQLite
    // version detection at runtime.
    let pool = setup_pool().await;
    let ctx = make_ctx("tid-batch", "oid-batch");
    upsert_user_from_auth(&pool, &ctx).await.unwrap();
    let key = ctx.principal_key();

    // Seed 50 ownership rows (fast, deterministic) and query for 500 ids
    // (250 hits + 250 misses) to exercise a realistically-sized batch.
    for i in 0..50 {
        set_ownership(
            &pool,
            "memory",
            &format!("m-{i}"),
            Some("agent-batch"),
            &key,
            Visibility::Personal,
            None,
        )
        .await
        .unwrap();
    }

    let mut ids: Vec<String> = (0..250).map(|i| format!("m-{i}")).collect();
    ids.extend((0..250).map(|i| format!("miss-{i}")));
    assert_eq!(ids.len(), 500, "500 binds plus 1 for resource_type = 501");

    let map = list_ownerships_by_ids(&pool, "memory", &ids).await.unwrap();
    assert_eq!(map.len(), 50, "only seeded rows resolve; misses are absent");
}

// ResourceScope parses the same three string forms as the query-param values
// the SPA will send. Mirrors the Visibility::parse pattern so the two enums
// stay aligned lexically.
#[test]
fn resource_scope_parses_all_variants() {
    assert_eq!(ResourceScope::parse("mine"), Some(ResourceScope::Mine));
    assert_eq!(ResourceScope::parse("team"), Some(ResourceScope::Team));
    assert_eq!(ResourceScope::parse("org"), Some(ResourceScope::Org));
    assert_eq!(ResourceScope::parse("personal"), None);
    assert_eq!(ResourceScope::parse("MINE"), None); // case-sensitive
    assert_eq!(ResourceScope::parse(""), None);
}

#[test]
fn resource_scope_round_trips_through_as_str() {
    for scope in [ResourceScope::Mine, ResourceScope::Team, ResourceScope::Org] {
        assert_eq!(ResourceScope::parse(scope.as_str()), Some(scope));
    }
}

#[tokio::test]
async fn list_resource_ids_owned_by_empty_principal_returns_empty() {
    let pool = setup_pool().await;
    let ctx = make_ctx("tid-empty", "oid-empty");
    upsert_user_from_auth(&pool, &ctx).await.unwrap();

    let ids = list_resource_ids_owned_by(&pool, &ctx.principal_key(), "memory")
        .await
        .expect("query runs against empty ownership table");
    assert!(ids.is_empty(), "no ownership rows means no ids");
}

#[tokio::test]
async fn list_resource_ids_owned_by_nonexistent_principal_returns_empty() {
    // A principal_key that never appeared in users or resource_ownership
    // must return an empty Vec, not an error. Handlers rely on this so an
    // unknown caller sees "no owned resources" rather than a 500.
    let pool = setup_pool().await;
    let ids = list_resource_ids_owned_by(&pool, "user:nonexistent@example.com", "memory")
        .await
        .expect("unknown principal is a valid query");
    assert!(ids.is_empty());
}

#[tokio::test]
async fn list_resource_ids_owned_by_returns_ids_for_owned_resources() {
    let pool = setup_pool().await;
    let ctx = make_ctx("tid-owner", "oid-owner");
    upsert_user_from_auth(&pool, &ctx).await.unwrap();
    let key = ctx.principal_key();

    for id in ["mem-1", "mem-2", "mem-3"] {
        set_ownership(
            &pool,
            "memory",
            id,
            Some("agent-a"),
            &key,
            Visibility::Personal,
            None,
        )
        .await
        .unwrap();
    }

    let mut ids = list_resource_ids_owned_by(&pool, &key, "memory")
        .await
        .unwrap();
    ids.sort();
    assert_eq!(
        ids,
        vec![
            "mem-1".to_string(),
            "mem-2".to_string(),
            "mem-3".to_string()
        ],
    );
}

#[tokio::test]
async fn list_resource_ids_owned_by_filters_by_resource_type() {
    // The same principal owns a memory and a project. A query for
    // resource_type="memory" returns only the memory id; the project id
    // must not leak. Guards against a future refactor that accidentally
    // drops the resource_type filter (e.g., building the wrong WHERE clause).
    let pool = setup_pool().await;
    let ctx = make_ctx("tid-mixed", "oid-mixed");
    upsert_user_from_auth(&pool, &ctx).await.unwrap();
    let key = ctx.principal_key();

    set_ownership(
        &pool,
        "memory",
        "mem-x",
        Some("agent-a"),
        &key,
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();
    set_ownership(
        &pool,
        "project",
        "proj-y",
        None,
        &key,
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();

    let memory_ids = list_resource_ids_owned_by(&pool, &key, "memory")
        .await
        .unwrap();
    assert_eq!(memory_ids, vec!["mem-x".to_string()]);

    let project_ids = list_resource_ids_owned_by(&pool, &key, "project")
        .await
        .unwrap();
    assert_eq!(project_ids, vec!["proj-y".to_string()]);
}

#[tokio::test]
async fn list_resource_ids_owned_by_excludes_other_principals() {
    // Two principals each own their own memory. A query from principal A
    // sees only A's id; B's remains private. This is the scope-filtering
    // core invariant the ResourceScope::Mine query-param relies on.
    let pool = setup_pool().await;
    let ctx_a = make_ctx("tid-a", "oid-a");
    let ctx_b = make_ctx("tid-a", "oid-b");
    upsert_user_from_auth(&pool, &ctx_a).await.unwrap();
    upsert_user_from_auth(&pool, &ctx_b).await.unwrap();

    set_ownership(
        &pool,
        "memory",
        "mem-a",
        Some("agent-a"),
        &ctx_a.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();
    set_ownership(
        &pool,
        "memory",
        "mem-b",
        Some("agent-b"),
        &ctx_b.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();

    let a_ids = list_resource_ids_owned_by(&pool, &ctx_a.principal_key(), "memory")
        .await
        .unwrap();
    assert_eq!(a_ids, vec!["mem-a".to_string()]);

    let b_ids = list_resource_ids_owned_by(&pool, &ctx_b.principal_key(), "memory")
        .await
        .unwrap();
    assert_eq!(b_ids, vec!["mem-b".to_string()]);
}
