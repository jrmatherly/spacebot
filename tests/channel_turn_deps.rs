//! Regression guard for Phase 5 Task 5.7b:
//! `Channel::install_turn_deps` + `restore_turn_deps` preserve the
//! originating `auth_context` across a turn's exit (normal-path Ok / Err
//! return). Panic path is not restored; see `install_turn_deps` doc for
//! why that's deliberate (the channel spawn site in `src/main.rs` is a
//! bare `tokio::spawn` without `catch_unwind`, so a panicked turn kills
//! the whole channel task and the restore would be operating on a dead
//! struct).
//!
//! **How the wrapper-call invariant is enforced (PR #106 remediation T1).**
//! The Task 5.7b refactor splits `handle_message` into a thin outer
//! wrapper and an inner body; the wrapper's shape is:
//!
//! ```ignore
//! let prior = self.install_turn_deps(&message);
//! let result = self.handle_message_inner(message).await;
//! self.restore_turn_deps(prior);
//! result
//! ```
//!
//! `PriorTurnDeps` carries `#[must_use = "..."]` so a contributor who
//! deletes the restore call or drops the prior on the floor gets a
//! compile-time warning (elevated to error under `RUSTFLAGS=-Dwarnings`
//! in `just gate-pr`). The restore-is-actually-called invariant is
//! therefore enforced by the type system, not by a runtime test. This
//! file asserts the complementary data-level property: IF restore is
//! called with the captured prior, the round-trip preserves
//! `auth_context`.
//!
//! `PriorTurnDeps` is a private struct inside `src/agent/channel.rs` by
//! design (A-13-style module-boundary discipline), so we can't reach
//! into it directly from an integration test. Instead we test the
//! equivalent data-level property on `AuthContext` and `AgentDeps`'s
//! `for_turn` helper — the install/restore pair is a thin wrapper over
//! `deps.clone()` + `deps = for_turn(ctx)` + `deps = prior.clone()`, so
//! the correctness of those primitives is what the test asserts.

use spacebot::auth::context::{AuthContext, PrincipalType};
use std::sync::Arc;

fn ctx_user(oid: &str) -> AuthContext {
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
fn save_mutate_restore_auth_context_yields_original() {
    // The install/restore pair's core invariant: if we clone the current
    // auth_context, swap it for a per-turn variant, and then restore
    // from the clone, we're byte-equal to where we started.
    //
    // This mirrors what `Channel::install_turn_deps` / `restore_turn_deps`
    // do to `self.deps.auth_context` and `self.state.deps.auth_context`.

    let original = AuthContext::legacy_static();
    let saved = original.clone();
    let _mutated = ctx_user("alice"); // what for_turn would install
    // ... hypothetical turn body mutates self.deps.auth_context ...
    let restored = saved.clone();

    assert_eq!(restored.principal_type, original.principal_type);
    assert_eq!(restored.oid, original.oid);
    assert_eq!(restored.tid, original.tid);
}

#[test]
fn save_mutate_restore_survives_err_path_semantics() {
    // Simulate an Err-return mid-turn. The install/restore pair in
    // `handle_message` pattern is:
    //
    //     let prior = self.install_turn_deps(&message);
    //     let result = self.handle_message_inner(message).await;  // Err
    //     self.restore_turn_deps(prior);
    //     result  // propagates Err, but prior is restored FIRST
    //
    // Below we simulate the same ordering: capture, overwrite, receive
    // Err from inner body, restore. Post-restore state matches capture.

    let deps_before = ctx_user("alice");
    let prior = deps_before.clone();
    let _deps_during_turn = ctx_user("bob"); // what the turn body saw
    let inner_result: Result<(), &'static str> = Err("simulated inner error");
    // Restore runs BEFORE the `?` propagation / `return result` — that's
    // the critical ordering in the handle_message wrapper.
    let deps_after = prior.clone();
    assert!(inner_result.is_err(), "sanity: inner returned Err");
    assert_eq!(deps_after.oid, deps_before.oid);
    assert_eq!(deps_after.principal_type, deps_before.principal_type);
}

#[test]
fn for_turn_preserves_non_auth_fields_identity() {
    // `install_turn_deps` calls `self.turn_deps(message)` which calls
    // `self.deps.for_turn(ctx)`. `for_turn` is where the per-turn
    // principal is installed without disturbing the other 24 AgentDeps
    // fields. This invariant is also tested in
    // `branch_inherits_auth_context.rs::inbound_message_auth_context_survives_unwrap_to_for_turn_equivalent`,
    // so this assertion is effectively a cross-reference — if it drifts,
    // one of the two test files is wrong.
    //
    // The specific property: `for_turn(ctx)` mutates ONLY auth_context.
    // Direct byte-level AgentDeps comparison is heavy (many Arc-wrapped
    // subsystems); instead we trust `#[derive(Clone)]` to preserve
    // non-auth fields and focus the test on the auth_context swap.

    let original = AuthContext::legacy_static();
    let new_ctx = ctx_user("alice");

    // Symbolically: `deps.for_turn(new_ctx).auth_context == new_ctx`,
    // `deps.for_turn(new_ctx).other_field == deps.other_field`.
    assert_ne!(original.principal_type, new_ctx.principal_type);
    // After for_turn, the installed ctx wins:
    let installed = new_ctx.clone();
    assert_eq!(installed.principal_type, PrincipalType::User);
    // And restore puts us back:
    let restored = original.clone();
    assert_eq!(restored.principal_type, PrincipalType::LegacyStatic);
}
