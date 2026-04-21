//! Integration tests for the cross-agent link policy from Phase 4.
//! `can_link_channel` is the gate for inter-agent message routing: the
//! acting principal must be able to read BOTH the calling agent and the
//! peer agent. Admins bypass via [`check_read`]'s admin check.

use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::policy::can_link_channel;
use spacebot::auth::principals::Visibility;
use spacebot::auth::repository::{set_ownership, upsert_user_from_auth};
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

fn user(oid: &str, roles: Vec<&str>) -> AuthContext {
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

#[tokio::test]
async fn owner_of_both_agents_can_link() {
    let pool = setup_pool().await;
    let alice = user("alice", vec![ROLE_USER]);
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
    set_ownership(
        &pool,
        "agent",
        "agent-b",
        None,
        &alice.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();

    let allowed = can_link_channel(&pool, &alice, "agent-a", "agent-b")
        .await
        .unwrap();
    assert!(allowed, "owner of both agents can link");
}

#[tokio::test]
async fn cannot_link_agents_owned_by_others() {
    let pool = setup_pool().await;
    let alice = user("alice", vec![ROLE_USER]);
    let bob = user("bob", vec![ROLE_USER]);
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
    set_ownership(
        &pool,
        "agent",
        "agent-b",
        None,
        &bob.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();

    let allowed = can_link_channel(&pool, &alice, "agent-a", "agent-b")
        .await
        .unwrap();
    assert!(
        !allowed,
        "alice must not link to bob's personal agent"
    );
}

#[tokio::test]
async fn org_visible_agents_link_freely() {
    let pool = setup_pool().await;
    let alice = user("alice", vec![ROLE_USER]);
    let bob = user("bob", vec![ROLE_USER]);
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
    set_ownership(
        &pool,
        "agent",
        "agent-b",
        None,
        &bob.principal_key(),
        Visibility::Org,
        None,
    )
    .await
    .unwrap();

    let allowed = can_link_channel(&pool, &alice, "agent-a", "agent-b")
        .await
        .unwrap();
    assert!(allowed, "org-visible target is linkable");
}

#[tokio::test]
async fn admin_can_link_anything() {
    let pool = setup_pool().await;
    let alice = user("alice", vec![ROLE_USER]);
    let bob = user("bob", vec![ROLE_USER]);
    let carol = user("carol", vec![ROLE_ADMIN]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    upsert_user_from_auth(&pool, &carol).await.unwrap();
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
    set_ownership(
        &pool,
        "agent",
        "agent-b",
        None,
        &bob.principal_key(),
        Visibility::Personal,
        None,
    )
    .await
    .unwrap();

    let allowed = can_link_channel(&pool, &carol, "agent-a", "agent-b")
        .await
        .unwrap();
    assert!(
        allowed,
        "admin link always allowed (and will be audited at handler)"
    );
}
