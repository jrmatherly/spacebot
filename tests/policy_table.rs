//! Table tests for the Phase 4 access policy. Uses the instance-DB
//! migrations from Phase 2 to construct realistic state. Each test builds
//! an in-memory SQLite pool, applies all `migrations/global`, seeds a
//! minimal principal + ownership shape, and asserts the policy decision.

use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::policy::{Access, DenyReason, check_read, check_write};
use spacebot::auth::principals::Visibility;
use spacebot::auth::repository::{set_ownership, upsert_team, upsert_user_from_auth};
use spacebot::auth::roles::{ROLE_ADMIN, ROLE_USER};
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

fn user(oid: &str, roles: Vec<&str>, groups: Vec<&str>) -> AuthContext {
    AuthContext {
        principal_type: PrincipalType::User,
        tid: Arc::from("tenant-1"),
        oid: Arc::from(oid),
        roles: roles.into_iter().map(Arc::from).collect(),
        groups: groups.into_iter().map(Arc::from).collect(),
        groups_overage: false,
        display_email: None,
        display_name: None,
    }
}

#[tokio::test]
async fn owner_can_read_personal_memory() {
    let pool = setup_pool().await;
    let alice = user("alice", vec![ROLE_USER], vec![]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    set_ownership(
        &pool,
        "memory",
        "m1",
        None,
        &alice.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();

    let decision = check_read(&pool, &alice, "memory", "m1").await.unwrap();
    assert!(
        matches!(decision, Access::Allowed),
        "owner should read own personal memory: got {decision:?}"
    );
}

#[tokio::test]
async fn non_owner_cannot_read_personal_memory() {
    let pool = setup_pool().await;
    let alice = user("alice", vec![ROLE_USER], vec![]);
    let bob = user("bob", vec![ROLE_USER], vec![]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    set_ownership(
        &pool,
        "memory",
        "m1",
        None,
        &alice.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();

    let decision = check_read(&pool, &bob, "memory", "m1").await.unwrap();
    assert!(
        matches!(decision, Access::Denied(DenyReason::NotYours)),
        "non-owner must be denied on personal memory: got {decision:?}"
    );
}

#[tokio::test]
async fn team_member_can_read_team_memory() {
    let pool = setup_pool().await;
    let alice = user("alice", vec![ROLE_USER], vec![]);
    let bob = user("bob", vec![ROLE_USER], vec![]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    let team = upsert_team(&pool, "grp-plat", "Platform").await.unwrap();
    // Bob is a team member; alice owns the resource and shares with the team.
    sqlx::query(
        r#"
        INSERT INTO team_memberships (principal_key, team_id, source)
        VALUES (?, ?, 'token_claim')
        "#,
    )
    .bind(bob.principal_key())
    .bind(&team.id)
    .execute(&pool)
    .await
    .unwrap();
    set_ownership(
        &pool,
        "memory",
        "m1",
        None,
        &alice.principal_key(),
        Visibility::Team,
        Some(&team.id),
    )
    .await
    .unwrap();

    let decision = check_read(&pool, &bob, "memory", "m1").await.unwrap();
    assert!(
        matches!(decision, Access::Allowed),
        "team member should read team memory: got {decision:?}"
    );
}

#[tokio::test]
async fn non_team_member_cannot_read_team_memory() {
    let pool = setup_pool().await;
    let alice = user("alice", vec![ROLE_USER], vec![]);
    let bob = user("bob", vec![ROLE_USER], vec![]); // not in the team
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    let team = upsert_team(&pool, "grp-plat", "Platform").await.unwrap();
    set_ownership(
        &pool,
        "memory",
        "m1",
        None,
        &alice.principal_key(),
        Visibility::Team,
        Some(&team.id),
    )
    .await
    .unwrap();

    let decision = check_read(&pool, &bob, "memory", "m1").await.unwrap();
    assert!(
        matches!(decision, Access::Denied(DenyReason::NotYours)),
        "non-team-member must be denied on team memory"
    );
}

#[tokio::test]
async fn anyone_can_read_org_memory() {
    let pool = setup_pool().await;
    let alice = user("alice", vec![ROLE_USER], vec![]);
    let bob = user("bob", vec![ROLE_USER], vec![]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    set_ownership(
        &pool,
        "memory",
        "m1",
        None,
        &alice.principal_key(),
        Visibility::Org,
        None,
    )
    .await
    .unwrap();

    let decision = check_read(&pool, &bob, "memory", "m1").await.unwrap();
    assert!(
        matches!(decision, Access::Allowed),
        "org visibility must allow all principals"
    );
}

#[tokio::test]
async fn admin_bypasses_personal_scoping() {
    let pool = setup_pool().await;
    let alice = user("alice", vec![ROLE_USER], vec![]);
    let carol = user("carol", vec![ROLE_ADMIN], vec![]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &carol).await.unwrap();
    set_ownership(
        &pool,
        "memory",
        "m1",
        None,
        &alice.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();

    let decision = check_read(&pool, &carol, "memory", "m1").await.unwrap();
    assert!(
        matches!(decision, Access::Allowed),
        "admin must bypass personal scoping"
    );
}

#[tokio::test]
async fn not_owned_returns_not_owned_reason() {
    let pool = setup_pool().await;
    let alice = user("alice", vec![ROLE_USER], vec![]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    // No set_ownership call; the resource has no row.

    let decision = check_read(&pool, &alice, "memory", "m-orphan")
        .await
        .unwrap();
    assert!(
        matches!(decision, Access::Denied(DenyReason::NotOwned)),
        "missing ownership row should yield NotOwned (handlers return 404 per backfill doc)"
    );
}

#[tokio::test]
async fn write_requires_ownership_even_for_team_visibility() {
    let pool = setup_pool().await;
    let alice = user("alice", vec![ROLE_USER], vec![]);
    let bob = user("bob", vec![ROLE_USER], vec![]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    let team = upsert_team(&pool, "grp-plat", "Platform").await.unwrap();
    sqlx::query(
        r#"
        INSERT INTO team_memberships (principal_key, team_id, source)
        VALUES (?, ?, 'token_claim')
        "#,
    )
    .bind(bob.principal_key())
    .bind(&team.id)
    .execute(&pool)
    .await
    .unwrap();
    set_ownership(
        &pool,
        "memory",
        "m1",
        None,
        &alice.principal_key(),
        Visibility::Team,
        Some(&team.id),
    )
    .await
    .unwrap();

    // Bob can READ (he's a team member) but cannot WRITE.
    let read = check_read(&pool, &bob, "memory", "m1").await.unwrap();
    assert!(matches!(read, Access::Allowed));
    let write = check_write(&pool, &bob, "memory", "m1").await.unwrap();
    assert!(
        matches!(write, Access::Denied(DenyReason::NotYours)),
        "team members can read but only the owner can write: got {write:?}"
    );
}

#[tokio::test]
async fn legacy_static_bypasses_all_checks() {
    let pool = setup_pool().await;
    let alice = user("alice", vec![ROLE_USER], vec![]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    set_ownership(
        &pool,
        "memory",
        "m1",
        None,
        &alice.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();

    let legacy = AuthContext::legacy_static();
    let decision = check_read(&pool, &legacy, "memory", "m1").await.unwrap();
    assert!(
        matches!(decision, Access::Allowed),
        "legacy static principal must bypass. It represents pre-Entra CI / scripts"
    );
}
