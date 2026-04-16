# Tooling Inventory

Complete map of MCP servers, hooks, agents, rules, and key permissions available in the Spacebot development environment. Load this reference when troubleshooting tool availability or when a session needs to understand what integrations are active.

## MCP Servers (15 Connected)

These MCP servers provide specialized tool capabilities beyond the built-in Claude Code tools.

### Code Intelligence

| Server | Transport | Purpose | Key Tools |
|--------|-----------|---------|-----------|
| **plugin:serena:serena** | UVX | Semantic codebase exploration — symbol lookup, find references, pattern search, memory management | `activate_project`, `find_symbol`, `find_referencing_symbols`, `get_symbols_overview`, `read_file`, `search_for_pattern`, `write_memory`, `read_memory`, `list_memories` |
| **plugin:code-review-graph:code-review-graph** | UVX | Structural knowledge graph — impact analysis, architecture overview, community detection | `build_or_update_graph_tool`, `semantic_search_nodes_tool`, `query_graph_tool`, `get_impact_radius_tool`, `get_review_context_tool`, `get_architecture_overview_tool`, `list_communities_tool` |
| **plugin:claude-mem:mcp-search** | Bun | Persistent cross-session memory — observation storage, semantic search, timeline, corpus building | `get_observations`, `smart_search`, `smart_outline`, `smart_unfold`, `search`, `timeline`, `build_corpus`, `query_corpus` |
| **sequential-thinking** | NPX | Step-by-step reasoning for complex problem decomposition | `sequentialthinking` |

### Documentation & Reference

| Server | Transport | Purpose | Key Tools |
|--------|-----------|---------|-----------|
| **plugin:context7:context7** | NPX | Fetch current library/framework documentation (React, Tokio, Rig, etc.) | `resolve-library-id`, `query-docs` |
| **plugin:microsoft-docs:microsoft-learn** | HTTP | Microsoft/Azure documentation search and fetch | `microsoft_docs_search`, `microsoft_docs_fetch`, `microsoft_code_sample_search` |
| **plugin:mintlify:Mintlify** | HTTP | Mintlify documentation search | `search_mintlify`, `query_docs_filesystem_mintlify` |
| **siderolabs-docs** | HTTP | Talos Linux and Omni documentation for cluster operations | `search_sidero_documentation`, `query_docs_filesystem_sidero_documentation` |

### Browser & UI Testing

| Server | Transport | Purpose |
|--------|-----------|---------|
| **plugin:playwright:playwright** | NPX | Browser automation — navigate, click, fill, screenshot, evaluate |
| **plugin:chrome-devtools-mcp:chrome-devtools** | NPX | Chrome DevTools — performance tracing, accessibility audits, console/network inspection |
| **chrome-devtools** | NPX | Duplicate Chrome DevTools instance (global config) |

### External Integrations

| Server | Transport | Purpose |
|--------|-----------|---------|
| **claude.ai Atlassian Rovo** | HTTP | Jira/Confluence — create issues, search, manage pages |
| **magic** | NPX | 21st.dev component builder and inspiration |
| **terraform** | Docker | Terraform/OpenTofu MCP for infrastructure operations |

### Auth-Pending (not typically active)

| Server | Purpose |
|--------|---------|
| **claude.ai Google Calendar** | Calendar management (requires OAuth) |
| **claude.ai Gmail** | Email management (requires OAuth) |

## Hooks (4 Active)

### Global Hooks (apply to all projects)

1. **UserPromptSubmit → `improve-prompt.py`**
   - Fires on every user message submission
   - Enriches prompts before execution
   - Bypasses: `*` prefix (raw), `/` prefix (slash commands), `#` prefix (memorize)
   - Location: `~/.claude/hooks/improve-prompt.py`

2. **PreToolUse(Read) → `claude-docs-helper.sh hook-check`**
   - Fires before every Read tool call
   - Documentation helper that gathers context about the file being read
   - Location: `~/.claude-code-docs/claude-docs-helper.sh`

### Project Hooks (spacebot-specific)

3. **PostToolUse(Edit|Write) → auto-format Rust**
   - Fires after any Edit or Write to a `.rs` file
   - Runs `cargo fmt --all` automatically
   - Keeps code formatted without manual intervention

4. **PreToolUse(Edit) → migration guard**
   - Fires before Edit on any file in `migrations/`
   - **Blocks the edit** with message: "Migration files are immutable. Create a new timestamped migration instead."
   - Enforces the append-only migration policy

## Custom Agents (2 Project-Level)

### migration-writer (Sonnet, max 10 turns)
- **Purpose:** Create new SQLite migrations
- **Tools:** Read, Grep, Glob, Write, Bash
- **Rules:** Never edit existing migrations, timestamp format YYYYMMDDHHMMSS, IF NOT EXISTS for idempotency, snake_case columns, created_at defaults
- **Invoke:** `Agent({ subagent_type: "migration-writer", prompt: "..." })`

### security-reviewer (Sonnet, read-only)
- **Purpose:** Review Rust code for security issues
- **Tools:** Read, Grep, Glob
- **Focus:** Secret handling (DecryptedSecret wrapper), unsafe blocks, error exposure, injection vectors, auth enforcement
- **Invoke:** `Agent({ subagent_type: "security-reviewer", prompt: "..." })`

## Project Rules (10 files in `.claude/rules/`)

These are referenced from CLAUDE.md and domain skills. Some are path-scoped, others apply to every change of a given kind.

| Rule | Applies To | Purpose |
|------|-----------|---------|
| `rust-essentials.md` | All Rust changes | Imports (3-tier), naming, error handling, comments, visibility, panics, logging |
| `rust-patterns.md` | Implementation reference | Struct derives, async patterns, trait design, serde, state machines, Rig integration |
| `rust-iteration-loop.md` | Every Rust change | Which check to run per change class (fmt, clippy, test, gate) |
| `coding-discipline.md` | All code work | Surface assumptions, simplicity, surgical edits, goal-driven TDD with docs/config/async escape hatches |
| `writing-guide.md` | Documentation and prose | Direct technical voice, banned patterns (em-dashes in prose, "Not X. Not Y." openers, banned words) |
| `async-state-safety.md` | `src/agent/**`, `src/messaging/**`, `src/tasks/**` | Race condition prevention, terminal states, idempotent termination |
| `messaging-adapter-parity.md` | `src/messaging/**/*.rs` | Behavior contracts across messaging backends |
| `provider-integration.md` | `src/llm/**`, `src/config/providers.rs`, `src/config/toml_schema.rs` | Config keys, resolution order, routing defaults, auth flows |
| `api-handler.md` | `src/api/**/*.rs` | Axum handler conventions |
| `tool-authoring.md` | `src/tools/**/*.rs` | Rig tool definition conventions |

## Environment Variables

```json
{ "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1" }
```

Enables multi-agent team features (parallel feature development, team-debug, team-review).

## Key Permission Patterns

The project allows over 100 permissions across global and project settings. Key categories:

- **All Serena tools** — symbol lookup, pattern search, memory read/write
- **All code-review-graph tools** — graph build, search, impact analysis
- **All claude-mem tools** — observation retrieval, smart search, outline
- **Build tools** — `cargo fmt/build/test/clippy/check/audit/doc/search/tree`
- **Frontend tools** — `bun install/run/test/audit/outdated/update/add`
- **Git operations** — `git commit/add/status/log/worktree`, `gh pr/issue/run/release`
- **Web access** — WebFetch, WebSearch
- **Cross-project reads** — `ai-k8s/**`, `ai-stack/**` directories
