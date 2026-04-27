//! Tests for `auth::scope::agent_ids_for`.
//!
//! Four cases cover the visibility paths the helper resolves:
//! - Admin sees every agent (legacy_static is admin-equivalent).
//! - Non-admin user sees only personally-owned agents.
//! - Non-admin user sees agents shared with a team they belong to.
//! - Non-admin user sees agents marked org-visible regardless of ownership.

use spacebot::auth::agent_ids_for;
use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::principals::Visibility;
use spacebot::auth::repository::{set_ownership, upsert_team, upsert_user_from_auth};
use spacebot::auth::roles::ROLE_USER;
use spacebot::db::DbPool;

use std::sync::Arc;

fn user_ctx(oid: &str) -> AuthContext {
    AuthContext {
        principal_type: PrincipalType::User,
        tid: Arc::from("t1"),
        oid: Arc::from(oid),
        roles: vec![Arc::from(ROLE_USER)],
        groups: vec![],
        groups_overage: false,
        display_email: None,
        display_name: None,
    }
}

async fn fresh_pool() -> Arc<DbPool> {
    let (_state, sqlite) = spacebot::api::ApiState::new_test_state_with_mock_entra().await;
    Arc::new(DbPool::Sqlite(sqlite))
}

async fn seed_agent(
    pool: &Arc<DbPool>,
    agent_id: &str,
    owner: &AuthContext,
    visibility: Visibility,
    team_id: Option<&str>,
) {
    set_ownership(
        pool,
        "agent",
        agent_id,
        None,
        &owner.principal_key(),
        visibility,
        team_id,
    )
    .await
    .unwrap();
}

/// Insert a team_memberships row directly. There is no `pub` helper for this
/// in `auth::repository`; production middleware writes it via raw SQL during
/// JWT-claim sync, and the test mirrors that path.
async fn add_team_membership(pool: &Arc<DbPool>, principal_key: &str, team_id: &str) {
    match pool.as_ref() {
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO team_memberships (principal_key, team_id, source) \
                 VALUES (?, ?, 'token_claim')",
            )
            .bind(principal_key)
            .bind(team_id)
            .execute(p)
            .await
            .unwrap();
        }
        DbPool::Postgres(_) => unreachable!("test uses SQLite fixture"),
    }
}

#[tokio::test]
async fn admin_sees_all_agent_ids() {
    let pool = fresh_pool().await;
    let alice = user_ctx("alice");
    let bob = user_ctx("bob");
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();

    seed_agent(&pool, "a-alice", &alice, Visibility::Personal, None).await;
    seed_agent(&pool, "a-bob", &bob, Visibility::Personal, None).await;

    // legacy_static() is admin-equivalent (is_admin returns true).
    let admin = AuthContext::legacy_static();
    let mut visible = agent_ids_for(&admin, &pool).await.unwrap();
    visible.sort();
    assert_eq!(visible, vec!["a-alice".to_string(), "a-bob".to_string()]);
}

#[tokio::test]
async fn user_sees_only_owned_personal_agents() {
    let pool = fresh_pool().await;
    let alice = user_ctx("alice");
    let bob = user_ctx("bob");
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();

    seed_agent(&pool, "a-alice", &alice, Visibility::Personal, None).await;
    seed_agent(&pool, "a-bob", &bob, Visibility::Personal, None).await;

    let visible = agent_ids_for(&alice, &pool).await.unwrap();
    assert_eq!(visible, vec!["a-alice".to_string()]);
}

#[tokio::test]
async fn user_sees_team_shared_agents() {
    let pool = fresh_pool().await;
    let alice = user_ctx("alice");
    let bob = user_ctx("bob");
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();

    // upsert_team derives `id = "team-<external_id>"`.
    upsert_team(&pool, "eng", "Engineering").await.unwrap();
    add_team_membership(&pool, &alice.principal_key(), "team-eng").await;

    // Bob owns a-bob and shares it with the engineering team. Alice is in
    // the team and should see it.
    seed_agent(&pool, "a-bob", &bob, Visibility::Team, Some("team-eng")).await;

    let visible = agent_ids_for(&alice, &pool).await.unwrap();
    assert_eq!(visible, vec!["a-bob".to_string()]);
}

#[tokio::test]
async fn user_sees_org_visible_agents() {
    let pool = fresh_pool().await;
    let alice = user_ctx("alice");
    let bob = user_ctx("bob");
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();

    // Bob owns a-bob with org visibility. Anyone authenticated should see it.
    seed_agent(&pool, "a-bob", &bob, Visibility::Org, None).await;

    let visible = agent_ids_for(&alice, &pool).await.unwrap();
    assert_eq!(visible, vec!["a-bob".to_string()]);
}
