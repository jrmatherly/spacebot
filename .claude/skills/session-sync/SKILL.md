---
name: session-sync
description: End-of-task sync — rebuild code graph, check CLAUDE.md for drift, sync Serena memories against codebase ground truth, and commit doc fixes. Run after completing any significant work (feature, migration, refactor, bug fix) to keep all documentation surfaces current. Use proactively when the user says "sync", "update docs", "session done", "wrap up", or finishes a multi-step task.
---

## End-of-Task Session Sync

Run this after completing any significant task to ensure all documentation surfaces stay current with the codebase.

### What this skill does (in order)

1. **Rebuild code-review graph** — incremental update to capture code changes
2. **Reflect on session learnings** — identify what's new, what changed, what future sessions need to know
3. **Check CLAUDE.md** — verify the project root CLAUDE.md is current (version, architecture summary, build commands, lint rules)
4. **Sync Serena memories** — read all 5 memories, identify drift against current codebase state, apply targeted edits
5. **Check roadmap** *(optional)* — if `docs/operations/roadmap.md` exists, verify it matches active OpenSpec changes
6. **Commit** — single commit covering all sync edits (doc/memory drift fixes only, not code changes)

### How to use

Invoke at the end of any session where you made substantive changes:

```
/session-sync
```

Or invoke with a scope hint:

```
/session-sync after adding WebSocket adapter + migration 043
```

The scope hint tells the skill what specifically to check for drift (version numbers, new modules, new migrations, changed tool counts, etc.).

### Behavioral rules

- **Read before writing** — always read the current state of each surface before proposing edits. Don't assume prior session's context is still accurate.
- **Targeted edits only** — fix what's stale, don't rewrite entire files. One-line fixes are preferred over paragraph rewrites.
- **Don't duplicate content** — CLAUDE.md references `@RUST_STYLE_GUIDE.md`, Serena memories summarize for session priming, the style guide is canonical. Each surface has a role.
- **Verify with ground truth** — before claiming a count (modules, migrations, tools, tests), grep or ls the codebase. Don't trust the existing memory value.
- **Exclude parallel session WIP** — if `git status` shows changes from another session's in-progress work, don't stage those files. Commit only your drift-sync edits.
- **Format check before commit** — run `cargo fmt --all -- --check` before committing to catch formatting drift.
- **Graceful degradation** — if a surface doesn't exist yet (no code-review graph, no roadmap), skip it and note the gap. Don't fail the sync.

### Surfaces to check

| Surface | Source of truth | What drifts |
|---|---|---|
| `CLAUDE.md` | Codebase + `RUST_STYLE_GUIDE.md` | Version, architecture summary, build commands, lint rules |
| `.serena/memories/project_overview` | `Cargo.toml` + `CLAUDE.md` | Version number, tech stack versions, process type list |
| `.serena/memories/project_structure` | `src/` directory | Module count, file descriptions, directory tree |
| `.serena/memories/style_and_conventions` | `RUST_STYLE_GUIDE.md` + codebase | Import ordering, naming, error handling patterns |
| `.serena/memories/suggested_commands` | `justfile` + `Cargo.toml` | Available just recipes, cargo commands, bun commands |
| `.serena/memories/task_completion_checklist` | `justfile` + CI | Gate commands, test counts, migration count |
| Code-review graph | `.code-review-graph/` | Node/edge counts, staleness |

### Step details

#### Step 1: Rebuild code-review graph

Check if the graph database exists:

```bash
ls .code-review-graph/ 2>/dev/null
```

- If it exists, run `/code-review-graph:build-graph` for an incremental update.
- If it doesn't exist, note that the graph needs initial setup and skip this step.

#### Step 2: Reflect on session learnings

Review what changed in this session. Consider:
- New modules or files added to `src/`
- New migrations added
- New tools added to `src/tools/`
- Config changes (Cargo.toml dependencies, justfile recipes)
- Architecture changes (new process types, new messaging adapters)

These observations inform what to check for drift in the following steps.

#### Step 3: Check CLAUDE.md

Read `CLAUDE.md` and verify against current codebase state:
- Version in Cargo.toml matches any version references
- Architecture section still accurate (process types, crate structure)
- Build commands still work (`just gate-pr`, `just preflight`)
- Import and lint sections match `RUST_STYLE_GUIDE.md` and `Cargo.toml` lints

#### Step 4: Sync Serena memories

For each of the 5 memories, use the Serena MCP tools:
1. `read_memory` to get current content
2. Compare against ground truth (grep, ls, read the actual files)
3. `edit_memory` for targeted fixes where drift is found

**What to verify per memory:**

- **project_overview**: version from `grep '^version' Cargo.toml`, process type list, tech stack versions
- **project_structure**: module count from `find src -name '*.rs' | wc -l`, directory tree accuracy
- **style_and_conventions**: still matches `RUST_STYLE_GUIDE.md` patterns
- **suggested_commands**: just recipes from `just --list --unsorted`, cargo/bun commands
- **task_completion_checklist**: migration count from `ls migrations/ | wc -l`, test counts, gate commands

#### Step 5: Check roadmap (optional)

Only if `docs/operations/roadmap.md` exists:

```bash
test -f docs/operations/roadmap.md && echo "exists" || echo "skip"
```

If it exists, verify active items match `openspec list --json` output and recently completed items are recorded.

#### Step 6: Commit

1. Run `cargo fmt --all -- --check` to verify no formatting drift
2. Stage only drift-sync files (CLAUDE.md, memory edits, roadmap updates)
3. Commit with convention: `docs: session sync — <what changed> + drift surface updates`

Do NOT stage code changes, only documentation and memory drift fixes.

### Quick verification commands

```bash
# Current version (should match memories)
grep '^version' Cargo.toml

# Per-agent migration count (should match memories)
ls migrations/*.sql | wc -l
# Total migrations across both tiers (per-agent + global)
find migrations -name '*.sql' | wc -l

# Integration test count
ls tests/*.rs | wc -l

# Source file count
find src -name '*.rs' | wc -l

# Tool count
ls src/tools/*.rs | wc -l

# Active OpenSpec changes
openspec list --json 2>/dev/null

# Just recipes available
just --list --unsorted 2>/dev/null | wc -l

# Serena memory list
# Use: mcp__plugin_serena_serena__list_memories
```

### Commit convention

```
docs: session sync — <what changed> + drift surface updates
```

Examples:
- `docs: session sync — WebSocket adapter + migration 043 + drift surface updates`
- `docs: session sync — provider routing refactor + drift surface updates`
- `docs: session sync — post-release v0.4.2 version bump + drift surface updates`

### Related

- `CLAUDE.md` — project root instructions, references `@RUST_STYLE_GUIDE.md`
- `RUST_STYLE_GUIDE.md` — canonical code style reference
- `.claude/rules/writing-guide.md` — voice/style constraints for docs and copy
- `.claude/skills/pr-gates/SKILL.md` — PR preparation workflow (run gates before push)
