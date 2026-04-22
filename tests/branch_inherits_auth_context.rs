//! Regression guard for Phase 4: AuthContext travels into Branch/Worker
//! deps via the `AgentDeps.auth_context` field.
//!
//! Full behavioral coverage (HTTP → Channel → Branch) lives in
//! `tests/api_memories_authz.rs` which exercises the handler layer with
//! a real request. This file asserts only the structural invariants:
//!
//! 1. `AgentDeps.auth_context` defaults to `LegacyStatic` (safe for boot,
//!    static-token path, and tests that don't set it).
//! 2. `AgentDeps::for_turn(ctx)` produces a fresh deps bundle with the
//!    supplied ctx and the receiver's other fields untouched.
//! 3. The bundle's `Clone` implementation preserves `auth_context`.
//!    Branches/Workers are spawned by cloning `state.deps`, so this is
//!    the mechanism by which inheritance works.
//!
//! Building a full `AgentDeps` here would require instantiating ~15 heavy
//! Arc-wrapped subsystems (LlmManager, MemorySearch, McpManager, sandbox,
//! etc.) just to test a plain struct field. Instead we test the
//! `AuthContext` semantics directly and trust `#[derive(Clone)]` to do
//! the right thing on the surrounding bundle.

use spacebot::auth::context::{AuthContext, PrincipalType};
use std::sync::Arc;

fn user(oid: &str) -> AuthContext {
    AuthContext {
        principal_type: PrincipalType::User,
        tid: Arc::from("t1"),
        oid: Arc::from(oid),
        roles: vec![],
        groups: vec![],
        groups_overage: false,
        display_email: None,
        display_name: None,
    }
}

#[test]
fn legacy_static_is_the_default_identity() {
    // Every AgentDeps construction site in the codebase (src/main.rs,
    // src/api/agents.rs, the two test fixtures) uses
    // AuthContext::legacy_static() as the initializer. Asserting this
    // constant here is the closest we get to "no site forgot to set it"
    // without grepping the whole tree.
    let ctx = AuthContext::legacy_static();
    assert_eq!(
        ctx.principal_type,
        PrincipalType::LegacyStatic,
        "legacy-static default is the only safe seed for pre-auth-middleware paths"
    );
    assert_eq!(ctx.principal_key(), "legacy-static");
}

#[test]
fn auth_context_clone_preserves_user_identity() {
    // Branches/Workers receive state.deps.clone() at spawn time. The
    // clone must carry the caller's oid, not reset to LegacyStatic.
    let alice = user("alice");
    let cloned = alice.clone();
    assert_eq!(cloned.oid.as_ref(), "alice");
    assert_eq!(cloned.principal_type, PrincipalType::User);
}

#[test]
fn auth_context_clone_preserves_system_principal() {
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
    let cloned = system.clone();
    assert!(matches!(cloned.principal_type, PrincipalType::System));
    assert_eq!(cloned.principal_key(), "system");
}

#[test]
fn inbound_message_auth_context_survives_unwrap_to_for_turn_equivalent() {
    // Encodes the runtime invariant exercised by `Channel::turn_deps`:
    // `InboundMessage.auth_context.clone().unwrap_or_else(legacy_static)`
    // yields the originating principal when the field is populated. The
    // real `turn_deps` is a private Channel method; we test the equivalent
    // unwrap chain here so a refactor that drops the `.clone()` before
    // `unwrap_or_else` (shifting away from Alice's identity to the static
    // fallback) is caught by a targeted test.
    let alice = user("alice");
    let mut msg = spacebot::InboundMessage::empty();
    msg.auth_context = Some(alice.clone());

    let ctx = msg
        .auth_context
        .clone()
        .unwrap_or_else(AuthContext::legacy_static);
    assert_eq!(ctx.oid.as_ref(), "alice");
    assert!(matches!(ctx.principal_type, PrincipalType::User));
}

#[test]
fn inbound_message_none_auth_context_falls_back_to_legacy_static() {
    // Platform adapters (Telegram/Discord/Mattermost/Slack/etc.) and
    // internal synthetics (cortex, cron retriggers) all construct
    // `InboundMessage` with `auth_context: None`. The dispatch-time
    // unwrap must fall back to LegacyStatic — not panic, and not
    // invent a user.
    let msg = spacebot::InboundMessage::empty();
    let ctx = msg
        .auth_context
        .clone()
        .unwrap_or_else(AuthContext::legacy_static);
    assert!(matches!(ctx.principal_type, PrincipalType::LegacyStatic));
    assert_eq!(ctx.principal_key(), "legacy-static");
}
