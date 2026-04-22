---
name: phase-5-audit-log-scaffolder
description: Use this skill to kick off Phase 5 (audit log, hash-chained + WORM export) of the Entra ID rollout. Ingests the Phase 5 plan at `.scratchpad/plans/entraid-auth/phase-5-audit-log.md`, walks the PR #104/#105 review artifacts for Phase-5-tagged deferred items, and produces the Task 5.1 seed + initial work plan. Spacebot-specific — only useful while the Entra rollout is in flight.
user-invocable: true
---

# Phase 5 Audit Log Scaffolder

## When to invoke

Invoke ONLY when starting Phase 5 work, on a fresh branch cut from `main`. The typical trigger: Phase 4 wrapped via `/entra-phase-wrap`, the resume file at `.scratchpad/session-primer/phase-5-resume.md` has been read, and the user is ready to start the audit-log implementation.

This is not a general-purpose skill. When Phase 5 ships, retire the skill (or repurpose for Phase 6).

## Preconditions

Before running any command, verify:

- [ ] Current branch matches `feat/entra-phase-5-audit-log` (or create it from `main`)
- [ ] `main` is at the squash commit of PR #105 (`bb9156c` or later); run `git log --oneline -1 main` to confirm
- [ ] `.scratchpad/plans/entraid-auth/phase-5-audit-log.md` exists and is unmodified since `main`
- [ ] `.scratchpad/session-primer/phase-5-resume.md` has been read in this session (context primer)
- [ ] No uncommitted changes in the working tree

Stop and ask the user if any precondition fails.

## What this skill produces

### 1. The Task 5.1 seed

A concrete commit-ready scaffold for the first structural piece of Phase 5: the `audit_events` migration + the `src/audit.rs` module root. Exact files:

- `migrations/global/20260420120007_audit_events.sql` — new migration
- `src/audit.rs` — module root with `pub mod types;` `pub mod appender;` `pub mod export;` declarations and a module-level `//!` doc block that names the architectural invariants
- Draft update to `src/lib.rs` adding `pub mod audit;`

The migration matches the plan's §"File structure" table. Specifically:

```sql
-- 20260420120007_audit_events.sql
CREATE TABLE IF NOT EXISTS audit_events (
    id BLOB PRIMARY KEY,          -- UUIDv7 (ordered) per plan §Architecture
    seq INTEGER UNIQUE NOT NULL,  -- monotonic; export cursor rides this
    prev_hash BLOB NOT NULL,      -- 32-byte SHA-256; genesis = all zeros
    row_hash BLOB NOT NULL,       -- 32-byte SHA-256 over canonical serialization
    timestamp TEXT NOT NULL,      -- RFC3339, UTC
    principal_key TEXT NOT NULL,  -- {tid}:{oid} or sentinel
    principal_type TEXT NOT NULL CHECK (principal_type IN ('user', 'service_principal', 'system', 'legacy_static')),
    action TEXT NOT NULL,
    resource_type TEXT,
    resource_id TEXT,
    decision TEXT NOT NULL CHECK (decision IN ('allowed', 'denied', 'admin_override', 'auth_success', 'auth_failure')),
    admin_override INTEGER NOT NULL DEFAULT 0 CHECK (admin_override IN (0, 1)),
    details_json TEXT              -- scrubbed via scrub_leaks per A-01
);

CREATE UNIQUE INDEX audit_events_seq_idx ON audit_events(seq);
CREATE INDEX audit_events_principal_idx ON audit_events(principal_key, timestamp);
CREATE INDEX audit_events_resource_idx ON audit_events(resource_type, resource_id, timestamp);

-- Track the last exported sequence so incremental export is crash-safe (A-14).
CREATE TABLE IF NOT EXISTS audit_export_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    last_exported_seq INTEGER NOT NULL DEFAULT 0,
    last_exported_at TEXT
);

INSERT OR IGNORE INTO audit_export_state (id, last_exported_seq) VALUES (1, 0);
```

### 2. Deferred-item inventory

A markdown table aggregating every Phase-5-scoped deferred item surfaced during PR #104 and PR #105 reviews. Source:
- `.scratchpad/2026-04-22-pr105-review-aggregated.md` (first-pass 5-agent review)
- `.scratchpad/2026-04-22-pr105-review-6-agent-delta.md` (second-pass 6-agent review)
- `.scratchpad/session-primer/phase-5-resume.md` § Deferred

Known Phase-5-tagged items from those artifacts:

| ID | Source | Description | Phase-5 task |
|---|---|---|---|
| I1 | PR #105 review | `reconcile_toml_agents_with_ownership` per-agent success trail missing | Task 5.4 (reconciliation audit events) |
| I2 | PR #105 review | `SendAgentMessageError` single-variant newtype; no retry-safe vs retry-wrong distinction | Task 5.7 (tool-call audit events with typed error categorization) |
| I3 | PR #105 review | HTTP create_agent orphan on `set_ownership` failure — no rollback/sweep | Task 5.8 (or deferred to Phase 10 orphan-sweep A-21) |
| I6 | PR #105 review | `serve_attachment` 404 collapses disk-missing with authz-denied | Task 5.9 (auth-decision audit event distinguishes the case) |
| N3 | PR #105 review | admin-override `tracing::info!` is log-only; no persistent record | Task 5.3 (REPLACE with AuditAppender::append) |
| N4 | PR #105 review | `install_turn_deps` scopeguard-restore for panic-window auth bleed | Task 5.10 (scopeguard RAII + audit event on abnormal turn exit) |
| N5 | PR #105 review | Settings hot-reload `let _ = tx.send(event)` swallows receiver-drop | Task 5.11 (audit event on settings-change failure) |
| A-19 follow-up | Phase 3 | photo-refresh failure events | Task 5.5 (Graph-sync failure audit events) |

### 3. Initial work plan

A concrete 3-commit sequence to kick off Phase 5:

**Commit 1**: migration + module scaffold
- Add migration + `src/audit.rs` root + `src/audit/types.rs` with `AuditEvent`, `AuditAction`, `AuditDecision` types
- `cargo clippy --lib --no-deps` must pass
- Tests: `cargo nextest run --test authz_data_model` (existing, should still pass)

**Commit 2**: `AuditAppender` with hash chaining
- Implement `src/audit/appender.rs` with the chained-insert mutex + `sha2::Sha256` canonical hashing per plan §Architecture
- New `tests/audit_chain.rs` with the 5 hash-integrity tests the plan § calls out

**Commit 3**: wire into `ApiState` and flip the N3 `tracing::info!` sites to `AuditAppender::append`
- `ApiState.audit_appender: Arc<ArcSwap<Option<Arc<AuditAppender>>>>` following the `task_store` / `wiki_store` setter pattern
- Flip all 14 admin-override `tracing::info!("admin_read override...")` sites in `src/api/*.rs` to `audit_appender.append(AuditEvent { decision: AdminOverride, ... })`
- The `tracing::info!` must also stay (dual emit) until the export path is proven out in Task 5.6

Commits 4-12+ follow the plan's task sequence.

## What this skill does

### Step 1: Precondition checks

Run the four checks listed above. Stop if any fails.

### Step 2: Read the plan

```bash
cat .scratchpad/plans/entraid-auth/phase-5-audit-log.md
```

Extract:
- The §"File structure" table (files to create + modify)
- The §"Phase 5 acceptance criteria" section
- The applicable amendment list from the top matter (A-01, A-02, A-03, A-13, A-14, A-15)

### Step 3: Read the review artifacts for deferred items

```bash
grep -E "^(I[0-9]|N[0-9])\b" .scratchpad/2026-04-22-pr105-review-aggregated.md \
                             .scratchpad/2026-04-22-pr105-review-6-agent-delta.md
```

Build the deferred-item inventory table shown above. Confirm each item's ID still matches the artifact.

### Step 4: Create the branch (if not already on it)

```bash
# From main
git checkout -b feat/entra-phase-5-audit-log
```

### Step 5: Produce the scaffold

Write the three files listed under §"The Task 5.1 seed" above. Do NOT commit yet — let the user review.

Show the user:
1. The migration SQL
2. The `src/audit.rs` module root
3. The planned updates to `src/lib.rs`
4. The deferred-item inventory
5. The 3-commit work plan

### Step 6: Hand off to `/superpowers:executing-plans`

Once the user approves the scaffold, they can commit Task 5.1 and run `/superpowers:executing-plans` with the Phase 5 plan as the spec. This skill's job is the kickoff; the plan-executor handles the rest.

## Amendments applicable (A-01, A-02, A-03, A-13, A-14, A-15)

The scaffolder embeds these invariants in the scaffolded files:

- **A-01**: `details_json` is populated via `crate::secrets::scrub::scrub_leaks(&details)`. Never call `scrub_secrets` (the old name). Never serialize bearer tokens into audit details.
- **A-02**: Test helpers follow the `ApiState::new_for_tests` pattern established in Phase 0; don't invent new test-state constructors.
- **A-03**: `SecretsStore::get_sync` for `AUDIT_S3_SECRET_KEY` retrieval at startup; async boundary stops at the load path.
- **A-13**: `AuditAppender::new` is `pub(crate)`. Production construction only via `ApiState::set_audit_appender`.
- **A-14**: `audit_export_state` migration lands in the same migration as `audit_events` so export cursor is present before the first row. The scaffolded migration above includes both tables.
- **A-15**: `export.rs` supports two runtime modes: `S3ObjectLock` and `HttpSiem`. Filesystem export is dev-only and gated behind `#[cfg(debug_assertions)]`. No "local file" production variant.

## Red flags — stop and ask

- `phase-5-audit-log.md` plan has been edited since `main` (local uncommitted plan amendments would desync the scaffold)
- The user is not on a fresh branch from `main`
- Any of the deferred-item IDs (I1-I8, N3-N5) don't match what the artifacts actually contain — report the drift
- A migration file with timestamp `20260420120007_*` already exists (either Phase 5 already started or the plan shifted numbers)

## What this skill does NOT do

- Does not implement `AuditAppender::append` — that's Task 5.2, after the scaffold lands
- Does not write the export path — Task 5.6
- Does not wire `AuditAppender` into middleware — Tasks 5.3 + 5.4
- Does not bump version or update CHANGELOG — the phase-wrap skill handles closure
- Does not run `just gate-pr` — too early; the scaffold won't pass a full gate until Task 5.2 lands the appender

## Composes with

- **`session-primer`**: run first to load Phase 5 context + read the resume file
- **`superpowers:writing-plans` / `superpowers:executing-plans`**: handle the per-task execution after this skill produces the seed
- **`pr-gates` + `entra-phase-wrap`**: Phase 5 ends at the wrap; this skill is the kickoff bookend
- **`write-path-sweep-verifier`**: add the Phase 5 pair family (`"audit_log append skipped: instance_pool not attached"`) to the subagent's registered families once it lands
