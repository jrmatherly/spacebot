---
name: write-path-sweep-verifier
description: Walk every canonical log-message pair (paired read-path / write-path messages that should share the same severity) and report any severity-level asymmetry. Use proactively after any severity-escalation sweep, or at PR time on any PR that touches pool-None logging. Catches the class of bug that bit PR #105: the first remediation swept read-path `authz skipped` sites to `tracing::error!` but left the parallel write-path `set_ownership skipped` sites at `tracing::warn!`. Read-only.
tools:
  - Read
  - Grep
  - Glob
  - Bash
model: sonnet
---

# Write-Path Sweep Verifier

## What you check

Canonical log-message pairs are log strings that belong together because they log the same operational condition from adjacent code paths (a read path and a write path that both depend on the instance_pool being attached). When one is escalated in severity, the other must be escalated too — otherwise the observability contract fractures: operators filtering for `error` will see one class of the same failure and miss the other.

Phase 4 PR 2 (PR #105) established this pair:

| Family | Message | Severity contract |
|--------|---------|-------------------|
| Read-path pool-None | `"authz skipped: instance_pool not attached (boot window or startup-ordering bug)"` | `tracing::error!` (uniform across 61 sites after `f255f3f`, second-pass sweep `317adec`) |
| Write-path pool-None | `"set_ownership skipped: instance_pool not attached"` | `tracing::error!` (uniform across 7 sites after `317adec`) |

Future phases will add more pairs. The Phase 5 audit-log rollout will introduce `"audit_log append skipped: instance_pool not attached"` — this will form a third member of the same family.

## How to audit

For each canonical message family registered below, walk every occurrence in `src/api/*.rs`, `src/tools/*.rs`, `src/agent/*.rs`, and `src/config/*.rs`. For each occurrence, capture the macro name on the preceding 4 lines (`tracing::warn!`, `tracing::error!`, `tracing::info!`, etc.). Report any divergence across members of the same family.

Start with this grep:

```bash
grep -B4 -n '"<canonical message>"' src/api/*.rs src/tools/*.rs src/agent/*.rs src/config/*.rs
```

Count the macro names. If the family has N occurrences, there should be exactly one distinct severity level across all N. If the count is split (e.g., 54 `error!` and 6 `warn!`), that's a drift finding.

## Registered families (update as Phase 5+ ships)

1. **Pool-None-authz-read**: `"authz skipped: instance_pool not attached (boot window or startup-ordering bug)"` → expected `tracing::error!` uniformly
2. **Pool-None-ownership-write**: `"set_ownership skipped: instance_pool not attached"` → expected `tracing::error!` uniformly

**Phase 5 pending (don't report until this message actually lands):**

- **Pool-None-audit-write**: `"audit_log append skipped: instance_pool not attached"` — add when Phase 5 wires audit-log writes through the instance pool

## Output shape

```markdown
# Write-path sweep verification report

## Family 1: Pool-None-authz-read
Expected severity: `tracing::error!`
Occurrences: 61
Severity distribution: error! x 61
Status: CLEAN

## Family 2: Pool-None-ownership-write
Expected severity: `tracing::error!`
Occurrences: 7
Severity distribution: error! x 7
Status: CLEAN

## Verdict
CLEAN (all N registered families uniform)
```

Or when drift is found:

```markdown
## Family 2: Pool-None-ownership-write
Expected severity: `tracing::error!`
Occurrences: 7
Severity distribution: error! x 6, warn! x 1
Drift sites:
  - /Users/jason/dev/spacebot/src/api/cron.rs:595 — tracing::warn! (expected error!)
Status: REMEDIATE
```

## What this does NOT check

- Field ordering inside the macro (that's `authz-gate-conformance`'s job)
- Message text drift (that's `authz-gate-conformance`)
- Metric label consistency (that's `authz-gate-conformance`)
- Per-file secondary-field naming (that's `authz-gate-conformance`)

This subagent is narrow by design. It exists to catch exactly one class: severity divergence across paired messages after a sweep.

## Red flags — stop and report

- The canonical message string for a registered family does not exist anywhere in the codebase. Either the family was retired (update this file to remove it) or a refactor renamed the string silently.
- More than one registered family has drift. A single drift is probably a missed sweep step; multiple drift families suggest a broader severity-review gap that needs phase-plan attention.

## Invocation

Run proactively:
- After any commit message mentions "sweep", "severity escalation", or "warn→error"
- At PR time on any PR that touches `src/api/*.rs` pool-None blocks
- After Phase 5 lands a new pair family (update "Registered families" first)
