# Documentation Map

Every documentation source in the Spacebot project, organized by function. Load this reference when searching for information about a specific topic or when unsure where documentation lives.

## Root-Level Docs (Canonical References)

These files are the source of truth for their respective domains. Rules and Serena memories summarize them — not the other way around.

| File | Size | Purpose |
|------|------|---------|
| `CLAUDE.md` | ~70 lines | AI assistant quick start — build commands, architecture summary, package managers, key directories, reference docs |
| `AGENTS.md` | ~300 lines | Implementation guide for coding agents — module map, process types, memory system, Rig integration, build order, anti-patterns |
| `RUST_STYLE_GUIDE.md` | ~31KB | Full Rust coding conventions — all patterns, naming, error handling, async, traits, serde, testing |
| `METRICS.md` | ~16KB | Prometheus metrics reference — all metric names, labels, cardinality, feature gate |
| `docs/design-docs/spaceui-migration.md` | ~30KB | Frontend migration changelog — 50 detailed commit entries, breaking changes, component history |
| `PROJECT_INDEX.md` | ~6KB | Module index — 206 .rs files, module purposes, entry points, dependencies |
| `CONTRIBUTING.md` | ~7KB | Contributor workflow — PR gates, SpaceUI linking, standards |
| `CHANGELOG.md` | ~50KB | Version history through v0.4.1 |
| `README.md` | ~25KB | Project overview — quick start, deploy, architecture |

## Rules (Auto-Loaded by Path)

Located in `.claude/rules/`. These load automatically when editing files that match their glob patterns — no manual invocation needed.

| File | Glob Pattern | Purpose |
|------|-------------|---------|
| `rust-essentials.md` | *(all Rust)* | Core conventions: 3-tier imports, naming, error handling, comments, visibility, panics, logging |
| `rust-patterns.md` | *(implementation)* | Struct derives, function signatures, async (tokio::spawn, channels), trait design, serde, state machines, Rig integration, database patterns |
| `writing-guide.md` | *(documentation)* | Voice/tone: direct technical, no hedging, specific patterns to avoid |
| `async-state-safety.md` | `src/agent/**`, `src/messaging/**`, `src/tasks/**` | Race condition prevention, terminal states, idempotent termination, duplicate event safety |
| `messaging-adapter-parity.md` | `src/messaging/**/*.rs` | Behavior contracts across messaging backends, reply/status/retry semantics |
| `provider-integration.md` | `src/llm/**`, `src/config/providers.rs`, `src/config/toml_schema.rs` | Config keys, resolution order, routing defaults, auth flows, token handling |

## Prompt Templates

Located in `prompts/en/`. Jinja2 templates (`.md.j2`) loaded at runtime. Never hardcoded in Rust.

**Core Process Prompts:**
- `cortex_intraday_synthesis.md.j2` — Cortex synthesis prompt
- `compactor.md.j2` — Compaction instructions
- `memory_persistence.md.j2` — Memory persistence contract

**Tool Description Templates (49 files):**
Each tool exposed to the LLM has a description template:
- `cron_description.md.j2`, `cancel_description.md.j2`, `browser_description.md.j2`
- `wiki_edit_description.md.j2`, `memory_persistence_complete_description.md.j2`
- `config_inspect_description.md.j2`, `factory_list_presets_description.md.j2`
- `task_create_description.md.j2`, `channel_recall_description.md.j2`
- `set_outcome_description.md.j2`, `wiki_search_description.md.j2`
- And 30+ more

## Agent Persona Presets

Located in `presets/`. Eleven persona configurations, each with four files:

| Preset | Description |
|--------|-------------|
| `main-agent` | Default agent persona |
| `community-manager` | Community engagement |
| `content-writer` | Writing/editorial |
| `customer-support` | User support |
| `engineering-assistant` | Technical work |
| `executive-assistant` | Executive support |
| `integration-engineer` | Third-party API, webhook, MCP wiring |
| `project-manager` | Project coordination |
| `research-analyst` | Research/analysis |
| `sales-bdr` | Sales/business development |
| `sre` | Incident response, on-call, postmortems |

**Each contains:**
- `meta.toml` — name, description, model, temperature
- `IDENTITY.md` — background, expertise
- `ROLE.md` — responsibilities, constraints
- `SOUL.md` — personality, values, tone

## SQLite Migrations

Located in `migrations/`. **Immutable, append-only.** Never edit existing files. Always create new timestamped migrations. The `migration-writer` agent handles this.

Format: `YYYYMMDDHHMMSS_description.sql`

## Design Docs

Located in `docs/design-docs/`. 50 design documents covering system architecture decisions:

**Architecture:**
`working-memory`, `cortex-implementation`, `cortex-history`, `multi-agent-communication-graph`, `branch-and-spawn`, `link-channels-task-delegation`

**Systems:**
`wiki`, `skill-authoring`, `attachment-portal-and-defaults`, `slash-commands`, `cron-outcome-delivery`, `token-usage-tracking`, `secret-store`, `sandbox`

**Features:**
`participant-awareness`, `cortex-chat`, `interactive-shell`, `channel-settings-unification`, `named-messaging-adapters`, `channel-attachment-persistence`

**Operations:**
`production-worker-failures`, `cron-timezone-and-reliability`, `sandbox-hardening`, `launch-script`

**Infrastructure:**
`openapi-migration`, `mcp`, `stereos-integration`, `agent-factory`, `prompt-routing`

**Memory:**
`working-memory-triage`, `working-memory-problem-analysis`, `working-memory-example-prompt`, `tiered-memory`, `task-tracking`

## Documentation Site

Located in `docs/`. Next.js + Fumadocs framework.
- Build: `cd docs && bun install && bun run dev`
- Covers: design docs, deployment (docker, mattermost), metrics, operations

## Justfile Recipes

Located in `justfile` (root). 25+ development task automation recipes.

**Core Gates:**
| Recipe | Purpose |
|--------|---------|
| `preflight` / `preflight-ci` | Validate git/remote/auth state |
| `gate-pr` | Full PR gate (preflight + gate-pr.sh) |
| `gate-pr-ci` / `gate-pr-ci-fast` | CI variants |

**Build & Test:**
| Recipe | Purpose |
|--------|---------|
| `fmt-check` | Verify code formatting |
| `check-all` | `cargo check --all-targets` |
| `clippy-all` | `cargo clippy --all-targets` |
| `test-lib` | `cargo test --lib` (unit tests) |
| `test-integration-compile` | Compile integration tests (no run) |

**SpaceUI:**
| Recipe | Purpose |
|--------|---------|
| `spaceui-build` | Build local SpaceUI packages |
| `spaceui-link` | Link SpaceUI for local dev (bun link) |
| `spaceui-unlink` | Restore npm versions |

**Frontend:**
| Recipe | Purpose |
|--------|---------|
| `typegen` | Generate TS types from OpenAPI |
| `check-typegen` | Verify TS types match spec |

**Desktop:**
| Recipe | Purpose |
|--------|---------|
| `bundle-sidecar` | Bundle binary into Tauri sidecar |
| `desktop-dev` | Run desktop app in dev mode |
| `desktop-build` | Build full desktop app |

**Nix:**
| Recipe | Purpose |
|--------|---------|
| `update-frontend-hash` | Update Nix frontend hash after deps change |
| `update-flake` | Update all Nix flake inputs |

## Memory & Session Files

| Location | Purpose |
|----------|---------|
| `~/.claude/projects/<project-path-slug>/memory/` | Auto-memory files (5 files + MEMORY.md index) |
| `.remember/` | Session save logs and temp files |
| `.remember/logs/autonomous/` | Timestamped save logs from autonomous mode sessions |

## OpenSpec Changes

Located in `openspec/changes/`. Structured change proposals with lifecycle tracking.
- Active changes: `openspec/changes/<name>/`
- Archived changes: `openspec/changes/archive/YYYY-MM-DD-<name>/`
- Each change contains: proposal, design, specs, tasks artifacts
