//! Integration tests for `SendAgentMessageTool`'s `can_link_channel`
//! enforcement (Phase 4 PR 2, T4.6 + T4.6b). The tool is the sole
//! cross-agent dispatch ingress; these tests verify the policy check fires
//! when the instance pool is attached and the skip path is exercised
//! cleanly when it isn't.

use arc_swap::ArcSwap;
use rig::tool::Tool;
use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::principals::Visibility;
use spacebot::auth::repository::{set_ownership, upsert_user_from_auth};
use spacebot::auth::roles::ROLE_USER;
use spacebot::conversation::history::ConversationLogger;
use spacebot::links::{AgentLink, LinkDirection, LinkKind};
use spacebot::tasks::TaskStore;
use spacebot::tools::{SendAgentMessageArgs, SendAgentMessageTool};
use sqlx::sqlite::SqlitePoolOptions;
use std::collections::HashMap;
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

fn user(oid: &str) -> AuthContext {
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

fn agent_names(pairs: &[(&str, &str)]) -> Arc<HashMap<String, String>> {
    Arc::new(
        pairs
            .iter()
            .map(|(id, name)| ((*id).to_string(), (*name).to_string()))
            .collect(),
    )
}

/// Seeded link between two agents so that `resolve_agent_id` has targets
/// to find. The policy check runs BEFORE link lookup, so presence of the
/// link does not affect the deny test; it only matters for the skip test
/// where we want to verify we proceed past the policy gate without
/// surfacing a "denied by policy" error.
fn link(from: &str, to: &str) -> Arc<ArcSwap<Vec<AgentLink>>> {
    Arc::new(ArcSwap::from_pointee(vec![AgentLink {
        from_agent_id: from.to_string(),
        to_agent_id: to.to_string(),
        direction: LinkDirection::TwoWay,
        kind: LinkKind::Peer,
    }]))
}

#[tokio::test]
async fn send_agent_message_denies_when_can_link_channel_denies() {
    let pool = setup_pool().await;
    let alice = user("alice");
    let bob = user("bob");
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    // agent-a is Alice's personal, agent-b is Bob's personal. Alice has
    // no path to agent-b, so `can_link_channel` returns false.
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

    let tool = SendAgentMessageTool::new(
        Arc::from("agent-a"),
        link("agent-a", "agent-b"),
        agent_names(&[("agent-a", "Alpha"), ("agent-b", "Beta")]),
        Arc::new(TaskStore::new(pool.clone())),
        ConversationLogger::new(pool.clone()),
        Some(pool.clone()),
        alice,
    );

    let err = tool
        .call(SendAgentMessageArgs {
            target: "agent-b".to_string(),
            message: "do a thing".to_string(),
        })
        .await
        .expect_err("alice must not be able to link agent-a -> bob's agent-b");
    let msg = err.to_string();
    assert!(
        msg.contains("denied by policy"),
        "error should surface policy denial, got: {msg}"
    );
}

#[tokio::test]
async fn send_agent_message_skips_policy_when_pool_none() {
    // No pool attached (pre-Phase-4 boot window). The tool must proceed
    // past the policy gate. The link-lookup step downstream will fail for
    // its own reasons (we've only seeded an agent-c link, not the requested
    // agent-b), but the error must NOT be "denied by policy".
    let pool = setup_pool().await;

    let tool = SendAgentMessageTool::new(
        Arc::from("agent-a"),
        link("agent-a", "agent-b"),
        agent_names(&[("agent-a", "Alpha"), ("agent-b", "Beta")]),
        Arc::new(TaskStore::new(pool.clone())),
        ConversationLogger::new(pool.clone()),
        None,
        AuthContext::legacy_static(),
    );

    let result = tool
        .call(SendAgentMessageArgs {
            target: "agent-b".to_string(),
            message: "do a thing".to_string(),
        })
        .await;

    // Either the tool succeeds (fully proceeds) or it errors for a
    // non-policy reason. Both are acceptable — the assertion is that we
    // made it past the policy gate.
    if let Err(err) = result {
        let msg = err.to_string();
        assert!(
            !msg.contains("denied by policy"),
            "pool=None must skip the policy check, got policy denial: {msg}"
        );
    }
}

fn system_ctx() -> AuthContext {
    AuthContext {
        principal_type: PrincipalType::System,
        tid: Arc::from(""),
        oid: Arc::from(""),
        roles: vec![],
        groups: vec![],
        groups_overage: false,
        display_email: None,
        display_name: None,
    }
}

#[tokio::test]
async fn send_agent_message_owner_of_both_agents_passes_policy_gate() {
    // Positive-path: Alice owns both agent-a (sender) and agent-b (target).
    // `can_link_channel` returns true. The policy gate passes; the tool
    // then proceeds to link lookup and downstream dispatch. We do NOT
    // assert full success here because the downstream path requires a
    // live messaging stack we do not build in this integration test.
    // The assertion is that the error, if any, is a non-policy error.
    let pool = setup_pool().await;
    let alice = user("alice");
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

    let tool = SendAgentMessageTool::new(
        Arc::from("agent-a"),
        link("agent-a", "agent-b"),
        agent_names(&[("agent-a", "Alpha"), ("agent-b", "Beta")]),
        Arc::new(TaskStore::new(pool.clone())),
        ConversationLogger::new(pool.clone()),
        Some(pool.clone()),
        alice,
    );

    let result = tool
        .call(SendAgentMessageArgs {
            target: "agent-b".to_string(),
            message: "do a thing".to_string(),
        })
        .await;

    // If the tool errors, the error must NOT be the policy denial. A
    // downstream infra error (no conversation manager, no messaging
    // stack) is acceptable proof that we passed the policy gate.
    if let Err(err) = result {
        let msg = err.to_string();
        assert!(
            !msg.contains("denied by policy"),
            "owner-of-both must pass policy, got policy denial: {msg}"
        );
    }
}

#[tokio::test]
async fn send_agent_message_system_principal_bypasses_policy() {
    // System-bypass: `is_admin` returns true for PrincipalType::System,
    // so `can_link_channel` composes two `check_read` calls that both
    // short-circuit allow. The policy gate must pass without requiring
    // ownership rows on either agent. Cortex retriggers and internal
    // adapters build `AuthContext::system()`-style contexts, and this
    // path must traverse link policy without surfacing "denied by policy".
    let pool = setup_pool().await;
    // Deliberately no ownership rows seeded: System bypass should not
    // depend on them.

    let tool = SendAgentMessageTool::new(
        Arc::from("agent-a"),
        link("agent-a", "agent-b"),
        agent_names(&[("agent-a", "Alpha"), ("agent-b", "Beta")]),
        Arc::new(TaskStore::new(pool.clone())),
        ConversationLogger::new(pool.clone()),
        Some(pool.clone()),
        system_ctx(),
    );

    let result = tool
        .call(SendAgentMessageArgs {
            target: "agent-b".to_string(),
            message: "do a thing".to_string(),
        })
        .await;

    if let Err(err) = result {
        let msg = err.to_string();
        assert!(
            !msg.contains("denied by policy"),
            "system-principal must bypass policy, got policy denial: {msg}"
        );
    }
}
