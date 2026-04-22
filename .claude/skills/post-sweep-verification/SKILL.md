---
name: post-sweep-verification
description: Use this skill after completing a bulk mechanical sweep (em-dash replacements, severity escalations, module-doc resyncs, grep-driven multi-file edits) that touched files across multiple gate boundaries. Maps modified file patterns to the specific verification gates that apply, then runs only the relevant gates. Encodes the CI-typegen lesson from PR #105, where an em-dash sweep via `perl -i -pe` on `src/api/*.rs` bypassed the PostToolUse typegen hook and landed stale `schema.d.ts` in CI.
user-invocable: true
---

# Post-Sweep Verification

## When to invoke

Invoke after completing a bulk multi-file sweep where:
- The edits were mechanical (grep-driven replacements, severity escalations, rename sweeps)
- The files touched cross gate boundaries (`src/api/*.rs` + `src/tools/*.rs` + `.claude/*.md` + `CHANGELOG.md`)
- You want to know which gates to run WITHOUT invoking `just gate-pr` (which is the pre-push gate, slower and broader than needed mid-sweep)

The signature failure this skill closes: at PR #105, the em-dash sweep in commit `530beb6` did `perl -i -pe` replacements on 79 sites across `src/api/*.rs`, then I ran only `cargo fmt --all` and `just check-fast`. I skipped `just check-typegen` because I treated the sweep as pure style. The utoipa-ingested route doc headers regenerated `schema.d.ts`, CI caught the drift, and a catch-up commit `bad98ff` was needed to close the PR. This skill's job is to prevent that class of post-sweep gate-skip.

## How it works

Given a list of modified files (or detect via `git diff --name-only HEAD`), map each file to the gate that applies, then run only the relevant gates once. Report pass/fail per gate.

### File-pattern → gate mapping

| File pattern | Gate that must run | Why |
|---|---|---|
| `src/api/**/*.rs` | `just check-typegen` | utoipa annotations regenerate `packages/api-client/src/schema.d.ts`. CI enforces this via the `check-typegen` job. |
| `**/*.rs` | `cargo fmt --all -- --check` | project-wide formatting invariant; the PostToolUse Edit-Write hook auto-fmts on Edit but shell-based edits bypass it |
| `**/*.rs` | `cargo clippy --lib --no-deps` | narrowest compile + lint; catches obvious ordering/scope breakage from a bulk sweep |
| `migrations/**/*.sql` | **BLOCK** | migrations are immutable; sweep should never have touched these. If it did, something went wrong upstream. |
| `prompts/**/*.md.j2` | no gate | Jinja2 templates rendered at runtime; writing-guide does not apply per project rules |
| `.claude/skills/**/*.md` | writing-guide grep | em-dash prose violations per `.claude/rules/writing-guide.md` |
| `.claude/agents/*.md` | writing-guide grep | same |
| `CHANGELOG.md` | writing-guide grep (lines added by this sweep only) | historical entries are out of scope; new content must comply |
| `CLAUDE.md` or `.claude/rules/**/*.md` | writing-guide grep | same |
| `interface/src/**/*.{ts,tsx}` | `cd interface && bun run typecheck` | TS type drift after string edits in types |
| `packages/api-client/src/*.{ts,d.ts}` | **BLOCK** | generated output; edit utoipa annotations instead |
| `spacedrive/**` | no gate (vendored fork) | Spacedrive has its own gate set; Spacebot's sweeps should not touch it |

## Execution sequence

### Step 1: Detect modified files

```bash
git diff --name-only HEAD
```

If there's nothing uncommitted, use the last commit's diff:

```bash
git show --name-only --pretty=format: HEAD | grep -v '^$'
```

### Step 2: Classify and deduplicate gates

Walk the file list. For each file, look up the gate pattern. Collect the distinct set of gates to run. Report the mapping to the user first so they can see what will happen.

### Step 3: Run gates in dependency order

Gate order matters: run formatting check first (fast), then the compile check, then the typegen check (slowest — 1-2 min for the openapi-spec rebuild).

```bash
# Always first if any .rs file touched
cargo fmt --all -- --check

# Second if any .rs file touched and the fmt check passed
cargo clippy --lib --no-deps

# Third if any src/api/**/*.rs file touched
just check-typegen

# Writing-guide grep on changed lines only (not the whole file)
git diff HEAD -- '**/*.md' ':(exclude)CHANGELOG.md' | grep -E '^\+[^+]' | \
    grep -nE '[a-z)"0-9]\s*—\s*[a-z]' || echo "no em-dash violations in added lines"
```

### Step 4: Report

Produce a verdict table showing each gate and its status. If any gate failed, surface the failure context from its stderr.

```markdown
## Post-sweep verification

### Files modified (12)
- src/api/memories.rs, tasks.rs, wiki.rs, cron.rs, portal.rs, agents.rs, notifications.rs, projects.rs, attachments.rs, ingest.rs
- CHANGELOG.md
- .claude/skills/handler-authz-rollout/SKILL.md

### Gates run

| Gate | Status | Triggered by |
|---|---|---|
| cargo fmt --all -- --check | PASS | `src/api/*.rs` x10 |
| cargo clippy --lib --no-deps | PASS | `src/api/*.rs` x10 |
| just check-typegen | FAIL | `src/api/*.rs` x10 |
| writing-guide grep | PASS | `.claude/skills/*.md`, `CHANGELOG.md` |

### Typegen drift
diff packages/api-client/src/schema.d.ts /tmp/spacebot-schema-check.d.ts
36c36
<         /** DELETE /foo — bar */
>         /** DELETE /foo: bar */
...

### Verdict
REMEDIATE — 1 gate failed. Run `just typegen` and commit the
regenerated `packages/api-client/src/schema.d.ts` before pushing.
```

## Gotchas

- **Do NOT run `just gate-pr`**. That's the full pre-push gate. This skill is a narrower, faster verification for mid-work checkpoints.
- **Do NOT run the unit-test suite**. Tests don't re-run on a style sweep (per `.claude/rules/rust-iteration-loop.md`: "Defer tests for style-only changes"). If the sweep was actually style-only, tests are redundant; if it had semantic content, `cargo clippy --lib` catches the compile-level regressions.
- **Writing-guide grep is on added lines only**. Historical content is out of scope. `git diff HEAD | grep '^+'` isolates the new content.
- **The typegen gate is slow** (1-2 min for the openapi-spec rebuild). Only run it when the file list justifies it; don't run it speculatively.

## Composes with

- **`pr-gates`**: the pre-push gate. This skill is a subset, run mid-work. `pr-gates` runs everything every time; `post-sweep-verification` runs only what the specific sweep needs.
- **`pr-remediation-batch`**: the remediation skill's Step 3 "verification between commits" invokes this skill per commit.
- **`authz-gate-conformance` + `code-doc-sync-auditor`**: after this skill verifies the compile/typegen gates, invoke these subagents for structural invariant checks.

## Invocation

```
/post-sweep-verification
```

or with an explicit file list:

```
/post-sweep-verification src/api/memories.rs src/api/tasks.rs
```
