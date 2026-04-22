---
name: handler-authz-rollout
description: Use this skill when adding a new REST handler under `src/api/` that needs the Phase-4 authz gate, OR when rolling the gate onto an existing ungated handler family. Scaffolds the ~45-line inline gate block (per N1 decision), the `AuthContext` extractor insertion (per api-handler.md ordering), the `.await set_ownership` on create paths (A-12), the pool-None fallback with file-family metric label, and the five canonical tests in `tests/api_<resource>_authz.rs`. Canonical references: `.claude/rules/api-handler.md` + `.scratchpad/plans/entraid-auth/phase-4-authz-helpers.md` (decision N1). Trigger: `/handler-authz-rollout` or when a PR adds a new `src/api/*.rs` handler that returns ownable data.
disable-model-invocation: true
---

# Handler Authz Rollout

## When to Use

Invoke when:

- Adding a new REST handler that returns or mutates an ownable resource (a resource with a row in `resource_ownership`)
- Rolling the Phase-4 authz gate onto an existing ungated handler file
- Phase 5+ audit-log handlers need the same gate pattern as the Phase-4 rollout
- Writing the canonical 5-test coverage shape for a new `tests/api_<resource>_authz.rs` file

Do NOT invoke for:

- Handlers that don't consult `resource_ownership` (e.g., `/api/health`, `/api/auth/config`, SSE streaming)
- Internal helper functions outside `src/api/`
- Tool-path authz (see `src/tools/send_agent_message.rs` for the tool-surface variant; use a different skill or hand-write)

## Canonical references

- `.claude/rules/api-handler.md` — extractor order, utoipa pattern, `ApiState` access
- `.scratchpad/plans/entraid-auth/phase-4-authz-helpers.md` — §"PR 2 deferred-item resolutions" row N1 pinning the inline decision
- `src/api/memories.rs` — reference proof-of-pattern (PR #104 + PR #105)
- `src/api/tasks.rs` — reference with pre-fetch-to-resolve-UUID pattern (task_number → task.id)

When this skill's scaffold disagrees with the reference handlers, the reference handlers win.

## The five invariants you are preserving

1. **N1 inline-at-each-call-site.** Do NOT extract a helper. The ~45-line block is copy-pasted to every gated handler. Review auditability beats DRY at this scale (< 15 handler files). Phase 5 may revisit once audit-log work lands.
2. **A-09 bare UUIDs.** `resource_id` arguments to `check_*` are bare UUIDs (or bare content_hash for `ingestion_file`). Never `format!("agent:{id}")` or similar prefix sigils.
3. **A-12 awaited set_ownership.** On create paths, `.await` the `set_ownership` call. NEVER `tokio::spawn` — fire-and-forget breaks the create-then-read UX.
4. **Metric label cardinality.** The `spacebot_authz_skipped_total{handler=<label>}` label is the **file family** (e.g., `"memories"`, `"tasks"`, `"wiki"`), NEVER a per-handler sub-label. Keeps cardinality flat.
5. **Extractor order.** `State → auth_ctx → Path/Query/Json` per `.claude/rules/api-handler.md`. Axum rejects at startup if sibling handlers in the same Router have mismatched orderings.

## Sequence

### Step 1: Collect inputs

From the user or the invocation context:

- **Resource name family** — determines `src/api/<family>.rs` and the test file `tests/api_<family>_authz.rs`. Plural lowercase (e.g., `audits`, `teams`, `invites`).
- **Resource type string** — singular, passed to `check_*`. Usually `<family>` minus the plural `s` (e.g., `"audit"`, `"team"`, `"invite"`). If it disagrees with the family (as with attachments→`"saved_attachment"`, ingest→`"ingestion_file"`), document in the module `//!` doc why.
- **Metric label** — the file-family name from step 1, verbatim.
- **Handlers in scope** — list every `pub(super) async fn` in the target file + classify read / write / create / list / unfiltered / special (e.g., `portal_send` auto-create, `cron` scheduled-run, `agents` TOML-reconciled).
- **Resource ID source** — how the handler gets `resource_id`: `Path<String>` direct UUID? `Path<i64>` pre-fetch-to-resolve (task_number pattern)? Slug fetch (wiki pattern)? Content hash (ingest pattern)?

### Step 2: Read the reference handler

Before writing anything, read `src/api/memories.rs:1-206` top to bottom. This is the canonical pattern. Every block in the target file mirrors this shape.

Then read `src/api/tasks.rs::get_task` (around line 313) for the pre-fetch-to-resolve-UUID variant if the handler uses that pattern.

### Step 3: Add the `AuthContext` extractor

For every gated handler, insert `auth_ctx: crate::auth::context::AuthContext` between `State(state)` and the remaining extractors:

```rust
pub(super) async fn get_<resource>(
    State(state): State<Arc<ApiState>>,
    auth_ctx: crate::auth::context::AuthContext,
    Path(id): Path<String>,
) -> Result<Json<...>, StatusCode> {
    // ...
}
```

List handlers that only gate when a filter argument is present use `_auth_ctx: crate::auth::context::AuthContext` (underscore prefix, unused warning suppression) so the middleware-auth check is asserted even without per-resource authz.

### Step 4: Inline the gate block (read handlers)

Template. Replace `<resource_type>`, `<resource_id>`, `<metric_label>` per file:

```rust
if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
    let (access, admin_override) =
        crate::auth::check_read_with_audit(&pool, &auth_ctx, "<resource_type>", &<resource_id>)
            .await
            .map_err(|error| {
                tracing::warn!(
                    %error,
                    actor = %auth_ctx.principal_key(),
                    resource_type = "<resource_type>",
                    resource_id = %<resource_id>,
                    "authz check_read_with_audit failed"
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    if !access.is_allowed() {
        return Err(access.to_status());
    }
    if admin_override {
        tracing::info!(
            actor = %auth_ctx.principal_key(),
            resource_type = "<resource_type>",
            resource_id = %<resource_id>,
            "admin_read override (audit event queued for Phase 5)"
        );
    }
} else {
    #[cfg(feature = "metrics")]
    crate::telemetry::Metrics::global()
        .authz_skipped_total
        .with_label_values(&["<metric_label>"])
        .inc();
    tracing::warn!(
        actor = %auth_ctx.principal_key(),
        <resource_id_field>_id = %<resource_id>,
        "authz skipped: instance_pool not attached (boot window or startup-ordering bug)"
    );
}
```

**For write handlers** — swap `check_read_with_audit` → `check_write`, drop the tuple-destructure (check_write returns `Access` directly, not `(Access, admin_override)`), drop the `admin_override` info log.

**Gate placement:** always BEFORE the first DB / store call. A gate placed after a store lookup leaks existence via timing or response shape.

### Step 5: Create handlers — A-12 set_ownership AFTER insert

```rust
// ... successful store.create returns new_id ...

if let Some(pool) = state.instance_pool.load().as_ref().as_ref().cloned() {
    crate::auth::repository::set_ownership(
        &pool,
        "<resource_type>",
        &new_id,
        None, // team_id — None means Personal visibility for single-user create
        &auth_ctx.principal_key(),
        crate::auth::principals::Visibility::Personal,
        None, // related_resource_id: usually None; portal_conversation uses this for parent
    )
    .await
    .map_err(|error| {
        tracing::error!(
            %error,
            <resource_type>_id = %new_id,
            "failed to register <resource_type> ownership"
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
} else {
    tracing::warn!(
        actor = %auth_ctx.principal_key(),
        <resource_type>_id = %new_id,
        "set_ownership skipped: instance_pool not attached"
    );
    #[cfg(feature = "metrics")]
    crate::telemetry::Metrics::global()
        .authz_skipped_total
        .with_label_values(&["<metric_label>"])
        .inc();
}
```

**Critical:** `.await` the `set_ownership`. NEVER wrap in `tokio::spawn`. If the insert succeeds but ownership registration fails via fire-and-forget, the creator's next GET races into 404.

**Portal-specific:** default visibility is `Visibility::Personal` per research §12 A-2 (portal conversations are private user chats). Extract a file-level `const PORTAL_VISIBILITY: Visibility = Visibility::Personal;` if the file has 2+ create sites.

### Step 6: Module-level `//!` doc

At the top of `src/api/<family>.rs`:

```rust
//! <Family> HTTP handlers + shared Phase-4 authz gate.
//!
//! Every <verb> handler consults `check_read_with_audit` /
//! `check_write` with `resource_type = "<resource_type>"` before
//! returning or mutating state. Access keys on <explain UUID
//! resolution; direct UUID if Path<String> carries it, fetch-and-
//! resolve if Path<i64> / Path<slug>>.
//!
//! The ~45-line inline gate block mirrors `src/api/memories.rs` per
//! Phase 4 PR 2 decision N1: single-file grep-visibility beats DRY.
//! Pool-None is always-on `tracing::warn!` plus feature-gated
//! `spacebot_authz_skipped_total{handler="<metric_label>"}`. Metric
//! label is the file resource family, never a per-handler sub-label,
//! which keeps cardinality flat.
//!
//! <file-specific quirks: Personal-default invariant, System-bypass,
//! slug→UUID indirection, parent-resource handling, etc.>
```

### Step 7: Test file. The five canonical gates

Create `tests/api_<family>_authz.rs`. Copy from `tests/api_tasks_authz.rs` (it has the most complete shape). Substitute `task` → `<family>` / `<resource_type>`. Required tests:

1. `non_owner_<verb>_<resource>_returns_404` — Alice owns, Bob attempts, expect 404
2. `owner_<verb>_<resource>_returns_200` — positive control
3. `admin_bypass_<resource>_read` — admin role bypasses
4. `create_<resource>_assigns_ownership` — reads ownership row back synchronously, asserts owner_principal_key and visibility
5. `pool_none_skip_<verb>_<resource>` — `ApiState::new_test_state_with_mock_entra_no_pool()`, expect non-401 / non-403

Use `ApiState::new_test_state_with_mock_entra()` + `build_test_router_entra(state)` + `mint_mock_token(&user)` throughout. No policy-module mocking; every test exercises the full middleware + policy + `resource_ownership` stack.

### Step 8: Verify

Per `.claude/rules/rust-iteration-loop.md` + INDEX § Cargo discipline:

```bash
# Compile the new test file on red (before implementing the handler gate)
cargo test --test api_<family>_authz --no-run 2>&1 | tail -10

# Between handler file edits (narrow type check)
cargo check --lib 2>&1 | tail -5

# On green (full test file)
cargo nextest run --test api_<family>_authz 2>&1 | tail -15

# If the handler has utoipa annotations (almost always yes)
just check-typegen

# Storage hygiene (if target/ > 40 GB)
du -sh target && just sweep-target
```

Do NOT run `just gate-pr` per-commit. Reserve it for pre-push per INDEX § Cargo discipline.

### Step 9: Commit

Commit message template:

```
feat(auth): roll authz gate to src/api/<family>.rs (Phase N T4.<X>)

<N> handlers gated with resource_type="<resource_type>" per the
Phase-4 pattern. Creates call .await set_ownership AFTER insert (A-12).
Pool-None fallback uniform with <reference-family-list> (always-on
tracing::warn! + metrics-feature-gated
spacebot_authz_skipped_total{handler="<metric_label>"}).

Module //! doc added. <file-specific notes — Personal-visibility
default, System-bypass contract, etc.>.

Tests: <N> new in tests/api_<family>_authz.rs covering owner-200,
non-owner-404, admin-bypass, create-ownership, pool-None skip. All
pass under nextest.
```

## What NOT to do

- Do NOT extract a helper function. The N1 decision is binding through Phase 9.
- Do NOT use per-handler metric sub-labels. Every gate in a file uses the same `handler=<family>` label.
- Do NOT use `tokio::spawn` around `set_ownership`. A-12 is non-negotiable.
- Do NOT prefix `resource_id` with `type:`. A-09 bare UUIDs (or bare content_hash for `ingestion_file`) only.
- Do NOT skip the module `//!` doc. The header is the audit trail for the N1 decision's file-level reasoning.
- Do NOT invoke if the handler doesn't consult `resource_ownership`. Authz-gating a stateless handler is pure overhead.

## Relationship to other automations

- `authz-gate-conformance` subagent: reviews drift between files this skill produced.
- `integration-test-coverage-auditor` subagent: reviews which canonical tests this skill's test file covered.
- `api-handler-add` skill: covers the utoipa + router-registration side; invoke it BEFORE this skill if the handler is brand-new (not just re-gating an existing one).
- `pr-remediation-batch` skill: picks up if this skill's output triggers reviewer findings that group into a remediation PR.
