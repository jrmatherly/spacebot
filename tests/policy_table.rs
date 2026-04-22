//! Table tests for the Phase 4 access policy. Uses the instance-DB
//! migrations from Phase 2 to construct realistic state. Each test builds
//! an in-memory SQLite pool, applies all `migrations/global`, seeds a
//! minimal principal + ownership shape, and asserts the policy decision.

use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::policy::{Access, DenyReason, check_read, check_read_with_audit, check_write};
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
async fn admin_read_sets_audit_flag_on_decision() {
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

    let (decision, admin_override) = check_read_with_audit(&pool, &carol, "memory", "m1")
        .await
        .unwrap();
    assert!(matches!(decision, Access::Allowed));
    assert!(
        admin_override,
        "admin reading another user's resource must flag admin_override"
    );
}

#[tokio::test]
async fn owner_read_does_not_set_audit_flag() {
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

    let (_decision, admin_override) = check_read_with_audit(&pool, &alice, "memory", "m1")
        .await
        .unwrap();
    assert!(!admin_override, "owner access is not break-glass");
}

#[tokio::test]
async fn valid_personal_ownership_non_owner_read_sees_not_yours() {
    // Documentary regression guard for Phase 4 PR #104 review finding I4.
    // The integrity branch at src/auth/policy.rs (invalid visibility string,
    // Team visibility with null shared_with_team_id) is UNREACHABLE through
    // the legitimate repository API: sqlite CHECK constraints enforce valid
    // `visibility` values on both INSERT and UPDATE, and set_ownership
    // accepts only a typed Visibility enum. The branch is defensive code
    // against DB tampering, migration skew, or sqlite pragma differences
    // that could bypass CHECK. No test can construct that state without
    // disabling CHECK, which is out of scope for integration tests.
    //
    // What this test DOES verify: the happy-path non-owner-on-Personal
    // flow (the same code region as the integrity branches) still produces
    // Access::Denied(NotYours) after the I4 refactor changed the integrity
    // branches to return anyhow::Err. The assertion guards against a
    // regression where the integrity refactor accidentally broke the
    // primary path.
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

    let result = check_read(&pool, &bob, "memory", "m1").await.unwrap();
    assert!(
        matches!(result, Access::Denied(DenyReason::NotYours)),
        "Valid Personal row: non-owner must see NotYours (not an integrity error)"
    );
}

#[tokio::test]
async fn legacy_static_read_does_not_set_audit_flag() {
    // LegacyStatic bypasses check_read via is_admin, but it represents
    // pre-Entra CI and scripts. Every HTTP request from that branch would
    // otherwise trip admin_override and pollute the quarterly access
    // review. admin_override must be gated on the SpacebotAdmin role, not
    // on is_admin(). Regression guard for the Phase 4 PR #104 review.
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
    let (decision, admin_override) = check_read_with_audit(&pool, &legacy, "memory", "m1")
        .await
        .unwrap();
    assert!(matches!(decision, Access::Allowed));
    assert!(
        !admin_override,
        "LegacyStatic bypass must NOT set admin_override"
    );
}

#[tokio::test]
async fn system_read_does_not_set_audit_flag() {
    // Same invariant for System (Cortex-initiated). Non-admin bypass path.
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

    let system = AuthContext {
        principal_type: PrincipalType::System,
        tid: Arc::from(""),
        oid: Arc::from(""),
        roles: vec![],
        groups: vec![],
        groups_overage: false,
        display_email: None,
        display_name: None,
    };
    let (decision, admin_override) = check_read_with_audit(&pool, &system, "memory", "m1")
        .await
        .unwrap();
    assert!(matches!(decision, Access::Allowed));
    assert!(!admin_override, "System bypass must NOT set admin_override");
}

#[tokio::test]
async fn admin_read_of_missing_resource_does_not_set_override() {
    // When the resource has no ownership row, check_read_with_audit
    // returns (Denied(NotOwned), false) BEFORE evaluating admin_override.
    // Nothing exists to break into, so no audit event should fire.
    // Regression guard: Phase 5's audit contract keys off admin_override
    // for the admin_read vs read event discriminator.
    let pool = setup_pool().await;
    let admin = user("admin-carol", vec![ROLE_ADMIN], vec![]);
    upsert_user_from_auth(&pool, &admin).await.unwrap();
    // NO set_ownership call: the resource has no row.

    let (decision, admin_override) = check_read_with_audit(&pool, &admin, "memory", "m-ghost")
        .await
        .unwrap();
    assert!(matches!(decision, Access::Denied(DenyReason::NotOwned)));
    assert!(
        !admin_override,
        "NotOwned must NOT set admin_override (nothing to break into)"
    );
}

#[tokio::test]
async fn admin_reading_other_admin_resource_sets_override() {
    // Phase 4 PR 1 Round-2 deferred finding G4. Two distinct admin
    // principals: carol (reader) and dave (owner). Both carry ROLE_ADMIN.
    // The audit flag MUST fire on carol's read of dave's resource —
    // admin-on-admin access is still a break-glass event per the role
    // matrix. `admin_bypasses_personal_scoping` (line 183) already covers
    // admin-reads-user-resource; this test fills the remaining gap so a
    // future narrowing of `admin_override` (e.g. "skip audit when owner
    // is also admin") would trip this explicit regression guard.
    let pool = setup_pool().await;
    let carol = user("carol", vec![ROLE_ADMIN], vec![]);
    let dave = user("dave", vec![ROLE_ADMIN], vec![]);
    upsert_user_from_auth(&pool, &carol).await.unwrap();
    upsert_user_from_auth(&pool, &dave).await.unwrap();
    set_ownership(
        &pool,
        "memory",
        "m1",
        None,
        &dave.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();

    let (decision, admin_override) = check_read_with_audit(&pool, &carol, "memory", "m1")
        .await
        .unwrap();
    assert!(
        matches!(decision, Access::Allowed),
        "admin reader must bypass per-resource ownership even when owner is also admin"
    );
    assert!(
        admin_override,
        "admin-on-admin access must still set admin_override — audit fires regardless of owner's role"
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
