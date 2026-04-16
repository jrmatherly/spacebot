# Memory Systems Guide

Spacebot development uses five complementary memory systems for cross-session continuity. Each serves a distinct purpose. Using the right system at the right time prevents "starting over" each session.

## System 1: Serena Project Memories

**What it is:** Structured project knowledge stored via the Serena MCP server. Five memories that encode architectural facts, conventions, and workflows.

**When to use:** Session startup (read all), session end (sync drift via `/session-sync`).

### Activation Sequence

```
1. mcp__plugin_serena_serena__activate_project({ project: "<project-root>" })  # pass cwd
2. mcp__plugin_serena_serena__check_onboarding_performed()
3. mcp__plugin_serena_serena__list_memories()
```

### The Five Memories

| Memory | Contents | Drifts When |
|--------|----------|-------------|
| `project_overview` | Version, tech stack, process types, database summary | Version bumps, new dependencies, architecture changes |
| `project_structure` | Module count, file descriptions, directory tree | New modules added, files renamed/removed |
| `style_and_conventions` | Import ordering, naming, error handling patterns | Style guide updates, new conventions adopted |
| `suggested_commands` | Just recipes, cargo/bun commands, common operations | New Justfile recipes, tool changes |
| `task_completion_checklist` | Migration count, test counts, gate commands | New migrations, test additions, CI changes |

### Reading a Memory

```
mcp__plugin_serena_serena__read_memory({ topic: "project_overview" })
```

### Updating a Memory (after verifying drift)

```
mcp__plugin_serena_serena__edit_memory({ topic: "project_overview", new_content: "..." })
```

### Key Rules
- **Always verify against ground truth** before updating (grep, ls, cargo commands)
- **Targeted edits only** — fix the stale line, don't rewrite the whole memory
- **Don't duplicate CLAUDE.md** — Serena memories summarize for quick priming; CLAUDE.md is canonical

---

## System 2: Claude-Mem (Cross-Session Observations)

**What it is:** Persistent observation database managed by the claude-mem plugin. Stores timestamped observations (discoveries, decisions, bug fixes, features) that survive across sessions. Semantic search over all past work.

**When to use:** Recall past decisions, find previous work on a topic, check if something was already attempted, build knowledge bases.

### Key Tools

| Tool | Purpose | When to Use |
|------|---------|-------------|
| `get_observations([ids])` | Fetch specific observations by ID (~300 tokens each) | When CMEM timeline shows relevant IDs |
| `smart_search(query)` | Semantic search across all observations | "Did we already fix the WebSocket reconnect?" |
| `smart_outline(file_path)` | Token-efficient structural view of a file | Before reading a large file — see its shape first |
| `smart_unfold(file_path, section)` | Expand a specific section from an outline | After smart_outline reveals the section of interest |
| `timeline(options)` | Chronological view of observations | Understanding sequence of past work |
| `search(query)` | Text-based observation search | When semantic search is too broad |
| `build_corpus(name)` | Build a queryable knowledge base from observations | For project-specific knowledge retrieval |
| `query_corpus(name, query)` | Query an existing corpus | Ask questions about accumulated knowledge |

### Observation Types (from CMEM header)

Each observation is tagged with a type emoji:
- `🎯` session — Session start/end markers
- `🔴` bugfix — Bug fixes applied
- `🟣` feature — New features added
- `🔄` refactor — Code refactoring
- `✅` change — Completed changes/tasks
- `🔵` discovery — Learnings, findings, root causes
- `⚖️` decision — Architectural or design decisions

### Reading the CMEM Header

The CMEM header is injected at the top of each session as a system reminder. It contains:
- Recent observation IDs with timestamps, types, and titles
- Total observation count and token statistics
- Instructions for accessing observations

### Efficiency Pattern

```
1. Read CMEM header (always present) — scan for relevant observation IDs
2. get_observations([relevant_ids]) — fetch ~300 tokens per observation
3. smart_search("topic") — if header doesn't have what's needed
4. smart_outline("file") → smart_unfold("file", "section") — progressive file reading
```

### Claude-Mem File-Read Gate (Important Gotcha)

The claude-mem plugin registers a `PreToolUse:Read` hook (file: `~/.claude/plugins/cache/thedotmack/claude-mem/<version>/hooks/hooks.json`) that rewrites Read tool calls for token economy. When all three conditions are true, the hook replaces the Read with `limit: 1` and injects a semantic timeline as context:

1. The target file is **≥ 1,500 bytes** (`FILE_READ_GATE_MIN_BYTES` in `src/cli/handlers/file-context.ts:20`)
2. The file has **prior claude-mem observations** for this project
3. The Read tool call passed **no explicit `offset` or `limit`**

**Symptoms:**
- `Read({file_path: "..."})` returns only line 1
- Hook-additional-context says "This file has prior observations. Only line 1 was read to save tokens."
- Suggests running `smart_outline()` or re-reading with `offset`/`limit`

**Workaround (for full reads):** Always pass an explicit `limit`. The hook checks `isTargetedRead = userOffset !== undefined || userLimit !== undefined` (line 186) and preserves any explicit value:

```
Read({file_path: "...", limit: 2000})   # reads full file if under 2000 lines
Read({file_path: "...", offset: 1, limit: 500})  # targeted read
```

**Scope:** Only affects the main-agent `Read` tool. Does NOT affect:
- Serena MCP tools (`read_memory`, `read_file`) — use these for full-file reads of architectural docs
- `cat`/`head`/`tail` via Bash — but prefer the dedicated tools
- Sub-agent Read calls (they run in isolated contexts without this hook)

**When this matters:**
- OpenSpec change artifacts (proposal.md, design.md, tasks.md, spec.md) — always pass `limit`
- Any file you've read or modified before in this project
- Reading code for review where you need full context

**Disabling entirely (not recommended):**
- Set `CLAUDE_MEM_EXCLUDED_PROJECTS` in `~/.claude-mem/settings.json` to include the project path
- Or remove the `PreToolUse` block from the plugin's `hooks.json` (lost on plugin update)

---

## System 3: Auto-Memory (Claude Code Built-in)

**What it is:** File-based memory in `~/.claude/projects/<project-path-slug>/memory/`. Claude Code automatically loads the `MEMORY.md` index at session start. The path slug is derived from the project's absolute path with slashes replaced by dashes.

**When to use:** Store durable user preferences, project facts, feedback corrections, and reference pointers that should persist indefinitely.

### Memory Types

| Type | Purpose | Example |
|------|---------|---------|
| `user` | User role, preferences, knowledge level | "Deep Rust expertise, prefers terse responses" |
| `feedback` | Corrections and confirmed approaches | "Integration tests must hit real DB, not mocks" |
| `project` | Ongoing work, goals, decisions with dates | "Merge freeze begins 2026-03-05 for mobile release" |
| `reference` | Pointers to external systems | "Pipeline bugs tracked in Linear project INGEST" |

### Current Memories

| File | Content |
|------|---------|
| `project_overview.md` | Spacebot: multi-process agentic system in Rust with Rig framework |
| `dev_workflow.md` | Justfile gates, cargo checks, bun for frontend |
| `spacebot_dev_skill.md` | 660-line skill encoding full architectural knowledge |
| `spacedrive_dev_skill.md` | 350-line skill for Spacedrive integration |
| `cluster_deployment_target.md` | Talos K8s cluster at ai-k8s/talos-ai-cluster |

### Key Rules
- **MEMORY.md is the index** — each entry is one line under 150 chars
- **Individual .md files hold the content** — with frontmatter (name, description, type)
- **Don't store code patterns** — those are derivable from the codebase
- **Don't store git history** — use `git log` / `git blame`
- **Verify before acting** — memory claims may be stale; check the file/function still exists

---

## System 4: Code-Review Graph

**What it is:** A Tree-sitter-powered structural knowledge graph of the codebase. Stores nodes (functions, structs, traits, modules) and edges (calls, imports, inherits). Updated incrementally via hooks and manual rebuild.

**When to use:** Understanding blast radius of changes, finding callers/callees, architecture exploration, code review context.

### Key Tools

| Tool | Purpose |
|------|---------|
| `build_or_update_graph_tool` | Rebuild/update the graph after code changes |
| `semantic_search_nodes_tool(query)` | Find functions/types by name or keyword |
| `query_graph_tool(pattern)` | Structural queries: `callers_of`, `callees_of`, `imports_of`, `importers_of`, `children_of`, `tests_for`, `inheritors_of`, `file_summary` |
| `get_impact_radius_tool(node)` | Blast radius analysis for a change |
| `get_review_context_tool(files)` | Token-efficient review context for specific files |
| `get_architecture_overview_tool` | High-level architecture map |
| `list_communities_tool` | Detect code communities/clusters |
| `list_flows_tool` / `get_flow_tool` | Trace execution flows |

### Usage Pattern

```
1. semantic_search_nodes_tool("memory_save") — find the function
2. query_graph_tool("callers_of:memory_save") — who calls it?
3. get_impact_radius_tool("memory_save") — what breaks if it changes?
4. Fall back to Grep/Glob/Read only when the graph doesn't cover what's needed
```

### Rebuild Trigger

The graph should be rebuilt after significant code changes:
- After feature implementation
- After refactoring
- As part of `/session-sync`

---

## System 5: .remember/ (Session Save Logs)

**What it is:** Session save logs in `.remember/` (project root). Contains autonomous mode session logs and temporary files. Managed by the `remember` plugin.

**When to use:** When investigating autonomous session execution history.

### Directory Contents

| Path | Purpose |
|------|---------|
| `logs/autonomous/save-*.log` | Timestamped logs from autonomous mode sessions |
| `tmp/` | Temporary session files |

### Access Pattern

Check `logs/autonomous/` for session save logs. For cross-session context retrieval, prefer claude-mem `smart_search` or `get_observations` over reading raw logs.

---

## Cross-Session Continuity Checklist

When starting a new session, retrieve context in this order:

1. **CMEM header** — already injected, scan for relevant recent observations
2. **Auto-memory MEMORY.md** — already loaded, provides durable project facts
3. **Serena activate + read memories** — architectural facts, conventions, commands
4. **Code-review graph** — if doing code work, ensure graph is current
5. **.remember/logs/autonomous/** — if investigating autonomous session history
6. **claude-mem smart_search** — if hunting for a specific past decision or discovery

When ending a session:

1. Run `/session-sync` to update all documentation surfaces
2. Key decisions/discoveries are automatically captured in CMEM observations
3. Auto-memory updates happen when the user provides feedback or preferences
