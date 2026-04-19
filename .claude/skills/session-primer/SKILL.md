---
name: session-primer
description: Session bootstrap primer for Spacebot development. This skill should be used when starting a new Claude Code session, when the user says "prime", "bootstrap", "get up to speed", "load context", "new session", "start fresh", "primer", "where did we leave off", "catch up", or "resume". Activates all memory systems, retrieves cross-session context, loads project knowledge, and prepares the assistant to continue work without re-learning. Typically the first skill invoked in any session.
---

# Session Primer

Bootstrap a new Claude Code session with full project context. This skill activates all memory and knowledge systems, retrieves cross-session continuity data, and prepares for productive work immediately.

## When to Invoke

- Start of every new session (before any task work)
- After a long gap between sessions
- When the user says "prime", "bootstrap", "get up to speed", or "new session"
- When context feels stale or incomplete

## Bootstrap Sequence

Execute these steps in order. Steps 1-3 are independent of each other and have no data dependencies. Steps 4-5 depend on earlier results.

> **⚠️ Read truncation gate.** The `claude-mem` plugin installs a `PreToolUse:Read` hook that truncates any `Read` call to 1 line when:
> (a) the target file is ≥ 1,500 bytes, AND
> (b) the file has prior claude-mem observations, AND
> (c) no explicit `offset` or `limit` was passed.
>
> **To bypass:** always pass an explicit `limit` parameter when reading files that may have prior observations. A large sentinel value like `limit: 2000` reads the whole file for most artifacts. This affects `Read` (the tool in this assistant), not Serena's `read_memory` or `read_file` MCP calls. See `references/memory-systems.md` → "Claude-Mem file-read gate" for the full mechanism.

### Step 1: Activate Serena Project Context

```
mcp__plugin_serena_serena__activate_project({ project: "<project-root>" })  # pass cwd
mcp__plugin_serena_serena__check_onboarding_performed()
```

Read all five Serena memories to load architectural facts:

```
mcp__plugin_serena_serena__read_memory({ topic: "project_overview" })
mcp__plugin_serena_serena__read_memory({ topic: "project_structure" })
mcp__plugin_serena_serena__read_memory({ topic: "style_and_conventions" })
mcp__plugin_serena_serena__read_memory({ topic: "suggested_commands" })
mcp__plugin_serena_serena__read_memory({ topic: "task_completion_checklist" })
```

### Step 2: Retrieve Cross-Session Context (parallel with Step 1)

Scan the CMEM header (the claude-mem timeline that appears as a system reminder at the start of each session) for recent observation IDs. Fetch the most recent observations to understand what was worked on last:

```
get_observations([<recent_observation_ids_from_header>])
```

If the user references previous work or asks "where did we leave off", also search:

```
smart_search("<topic from user's question>")
```

### Step 3: Assess Current State (parallel with Steps 1-2)

Run these in parallel to understand the workspace:

```bash
git status                    # Working tree state
git log --oneline -10         # Recent commits
git branch --show-current     # Current branch
```

Check for active OpenSpec changes:

```bash
ls openspec/changes/ 2>/dev/null   # Active change proposals
```

### Step 4: Identify the Task Domain

Based on the user's request (or lack of one), determine which domain skill to recommend:

| Signal | Skill to Suggest |
|--------|-----------------|
| Rust code changes in `src/` | `/spacebot-dev` |
| Spacedrive integration | `/spacedrive-dev` |
| UI components or frontend | `/spaceui-dev` |
| Kubernetes or deployment | `/cluster-context` |
| Dependency updates | `/deps-update` |
| OpenSpec change work | `/openspec-apply-change` or `/openspec-explore` |
| PR preparation | `/pr-gates` |
| End of session | `/session-sync` |

Do not load domain skills preemptively. Recommend them when the task becomes clear.

### Step 5: Report Readiness

Provide a concise status report:

```
Project: Spacebot
Branch: <current branch>
Working tree: <clean/dirty>
Last work: <summary from CMEM observations>
Serena memories: <loaded count>
Ready for: <inferred task domain or "awaiting task">
```

## What This Project Is

Spacebot is a multi-process agentic AI system built in Rust with the Rig framework. Single binary crate, three embedded databases (SQLite, LanceDB, redb), five process types (Channel, Branch, Worker, Compactor, Cortex). See `PROJECT_INDEX.md` for current module counts and `AGENTS.md` for the full architecture guide.

**Package managers:** `cargo` for Rust, `bun` for frontend (never npm/pnpm/yarn).

**Key gates:** `just preflight` before finalizing, `just gate-pr` before pushing. If same command fails twice, stop and debug.

## Memory Systems Quick Reference

Four primary memory systems provide cross-session continuity, plus session logs. Use the right one for the situation:

| System | Purpose | Access Pattern |
|--------|---------|---------------|
| **Serena** | Architectural facts, conventions, commands | `activate_project` → `read_memory` (5 memories) |
| **Claude-Mem** | Past decisions, discoveries, bug fixes | CMEM header → `get_observations` → `smart_search` |
| **Auto-Memory** | Durable user preferences, project facts | Auto-loaded via `MEMORY.md` index |
| **Code-Review Graph** | Structural code relationships, impact analysis | `semantic_search_nodes_tool` → `query_graph_tool` |
| **.remember/** | Session save logs (autonomous mode) | Check `logs/autonomous/` for session history |

For detailed usage patterns, consult **`references/memory-systems.md`**.

## Key MCP Servers

Four MCP servers are essential for daily work:

| Server | Purpose | When to Use |
|--------|---------|-------------|
| **Serena** | Symbol lookup, references, pattern search, project memories | Code navigation, understanding architecture |
| **Code-Review Graph** | Impact analysis, architecture overview, structural queries | Before making changes, during code review |
| **Claude-Mem** | Cross-session observations, semantic search | Recalling past work, checking if something was tried |
| **Context7** | Library documentation (Tokio, Rig, Serenity, etc.) | API questions, version migration |

For the full tooling inventory, consult **`references/tooling-inventory.md`**.

## Superpowers Skills (Critical Workflow Skills)

The superpowers plugin provides essential workflow skills that govern how work gets done. Invoke these at the right moment:

| Skill | When to Invoke |
|-------|---------------|
| `/superpowers:using-superpowers` | Session start (auto-loaded, establishes skill discovery) |
| `/superpowers:brainstorming` | Before any creative work, new features, or design decisions |
| `/superpowers:writing-plans` | Before multi-step tasks, when a spec or requirements exist |
| `/superpowers:executing-plans` | When executing a written plan in a new session |
| `/superpowers:writing-skills` | When creating or editing skills |
| `/superpowers:using-git-worktrees` | Before feature work that needs isolation |
| `/superpowers:systematic-debugging` | Before proposing fixes for any bug or test failure |
| `/superpowers:test-driven-development` | Before writing implementation code for features or fixes |
| `/superpowers:verification-before-completion` | Before claiming work is done or creating PRs |
| `/superpowers:requesting-code-review` | After completing features, before merging |
| `/superpowers:receiving-code-review` | When processing review feedback |
| `/superpowers:finishing-a-development-branch` | When implementation is complete and tests pass |
| `/superpowers:dispatching-parallel-agents` | When facing 2+ independent tasks |
| `/superpowers:subagent-driven-development` | When executing plans with independent tasks |

These are not optional conveniences. They encode disciplined workflows that prevent rework and catch issues early. When a superpowers skill applies, invoke it.

## Project Skills (20 Others)

Skills are invoked as slash commands. Three tiers matter most:

**Architecture** (activate when doing code work):
- `/spacebot-dev` — Spacebot internals (process types, memory, tools, Rig integration)
- `/spacedrive-dev` — Spacedrive integration (VDFS, sync, P2P, extensions)
- `/spaceui-dev` — UI component library (tokens, primitives, forms, ai, explorer, icons)

**Workflow** (activate at the right lifecycle moment):
- `/session-sync` — end-of-task documentation sync
- `/pr-gates` — pre-push validation
- `/deps-update` — dependency audit and update
- `/commit-all` — group and commit changes

**OpenSpec** (structured change management):
- `/openspec-explore` → `/openspec-propose` → `/openspec-apply-change` → `/openspec-verify-change` → `/openspec-archive-change`

For the complete catalog with triggers and composition patterns, consult **`references/skills-catalog.md`**.

## Documentation Locations

| Need | Location |
|------|----------|
| AI assistant context | `CLAUDE.md` (root) |
| Agent implementation guide | `AGENTS.md` |
| Rust conventions | `RUST_STYLE_GUIDE.md` |
| Module index | `PROJECT_INDEX.md` |
| Metrics reference | `METRICS.md` |
| Design decisions | `docs/design-docs/` (50 docs) |
| Prompt templates | `prompts/` (88 Jinja2 files, 51 tool descriptions) |
| Agent presets | `presets/` (11 personas) |
| Just recipes | `justfile` (41 recipes) |

For the complete documentation map, consult **`references/documentation-map.md`**.

## Hooks to Know About

Four hooks run automatically — two are project-specific:

1. **Post-Edit/Write on `.rs` files** → auto-runs `cargo fmt --all`
2. **Pre-Edit on `migrations/`** → blocks the edit (migrations are immutable)
3. **UserPromptSubmit** → enriches prompts (bypass with `*` prefix)
4. **Pre-Read** → documentation context gathering

## Custom Agents

Two project-level agents for delegation:

- **migration-writer** — creates new timestamped SQLite migrations (Sonnet, 10 turns)
- **security-reviewer** — reviews Rust code for security issues (Sonnet, read-only)

## Behavioral Rules

- **Read before writing** — always read current state before proposing changes
- **Verify against ground truth** — don't trust memory values without checking the codebase
- **Targeted edits** — fix what's stale, don't rewrite whole files
- **Bun only** — never npm, pnpm, or yarn for frontend work
- **Migration immutability** — never edit existing migration files
- **Gate discipline** — `just preflight` + `just gate-pr` before every push

## Additional Resources

### Reference Files

For detailed information beyond this primer:
- **`references/tooling-inventory.md`** — Complete map of MCP servers, hooks, agents, rules, permissions
- **`references/memory-systems.md`** — How to use all five memory systems for cross-session continuity
- **`references/skills-catalog.md`** — All 22 project skills with triggers and composition patterns
- **`references/documentation-map.md`** — Every documentation source, location, and purpose
