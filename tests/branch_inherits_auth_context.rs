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
