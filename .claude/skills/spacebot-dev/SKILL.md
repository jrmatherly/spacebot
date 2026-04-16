---
name: spacebot-dev
description: Specialized Spacebot development skill with deep architectural knowledge. Use when working on any Spacebot feature, bug fix, refactor, or investigation. Covers the five-process agent model (Channel, Branch, Worker, Compactor, Cortex), memory system (SQLite + LanceDB + redb), task lifecycle, 60+ tool definitions, Rig framework integration, prompt templates, identity system, messaging adapters, config hot-reload, security/sandbox, and API server. Activate for any Rust code changes in src/, prompt template edits in prompts/, migration work, tool additions, or architectural questions.
---

# Spacebot Development Guide

You are working on Spacebot, a multi-process agentic AI system built in Rust. This skill gives you the architectural knowledge to make correct decisions without reading every file.

## Architecture Overview

Single binary crate (`src/lib.rs` + `src/main.rs`). No workspace. 34 public modules. Three embedded databases. Five process types. 60+ LLM tools. 9 messaging platform adapters (including Portal).

The core abstraction: every LLM process is a Rig `Agent<SpacebotModel, SpacebotHook>` with an isolated `ToolServer`, independent history, and typed `ProcessEvent` communication.

## The Five Process Types

Each process type has a distinct role, tool set, and isolation boundary. Never mix their responsibilities.

### Channel (`src/agent/channel.rs`)
- User-facing conversation. Delegates everything. Never blocks.
- Tools: `reply`, `branch`, `spawn_worker`, `route`, `cancel`, `skip`, `react`, `cron`, `send_file`, `send_message`, `project_manage`
- Conditional tools: `attachment_recall` (when save_attachments), `send_agent_message` (when links configured), `set_outcome` (cron executions)
- NO memory tools (delegated to branches)
- Max turns: 5. Model default: claude-sonnet
- Sub-modules: `channel_attachments.rs`, `channel_dispatch.rs`, `channel_history.rs`, `channel_prompt.rs`
- Key struct: `Channel { id, agent_id, history, compactor, control_handle, ... }`

### Branch (`src/agent/branch.rs`)
- Fork of channel context for independent thinking. Short-lived (seconds).
- Tools: `memory_save`, `memory_recall`, `memory_delete`, `channel_recall`, `spacebot_docs`, `email_search`, `worker_inspect`, `task_create`, `task_list`, `task_update`, `file_read`
- Conditional tools: wiki tools (6, when wiki enabled), `memory_persistence_complete` (for persistence contract), `spawn_worker` (when channel state available)
- Max turns: 10. Model default: claude-sonnet
- Memory persistence contract: silent branches must save memories before completing
- Constants: `MAX_OVERFLOW_RETRIES: 2`, `MAX_MEMORY_CONTRACT_RETRIES: 2`

### Worker (`src/agent/worker.rs`)
- Independent task executor. No channel context. Has execution tools.
- Tools: `shell`, `file_read`, `file_write`, `file_edit`, `file_list`, `task_update`, `set_status`, `worker_inspect`, optionally `browser`, `web_search`, `mcp_*`
- Max turns: 50 (segmented into 15-turn blocks via `TURNS_PER_SEGMENT`)
- States: `Running`, `WaitingForInput`, `Done`, `Failed`
- Two kinds: fire-and-forget and interactive (accepts follow-ups via `route`)
- Constants: `MAX_OVERFLOW_RETRIES: 2`, `MAX_TRANSIENT_RETRIES: 5`, `MAX_SEGMENTS: 10`

### Compactor (`src/agent/compactor.rs`)
- Programmatic monitor, NOT an LLM process. Watches context size, triggers compaction workers.
- Thresholds: 80% background, 85% aggressive, 95% emergency truncation
- Called via `check_and_compact()` after each channel turn
- Actions: `Background`, `Aggressive`, `EmergencyTruncate`

### Cortex (`src/agent/cortex.rs`)
- System-level observer. Singleton per agent. Sees across all channels.
- Tools (bulletin): `memory_save` only
- Tools (cortex chat): full superset — memory + task + shell + file + browser + factory + config tools
- Responsibilities: memory bulletin, worker/branch supervision, memory maintenance, ready-task loop
- Constants: `BULLETIN_REFRESH_FAILURE_BACKOFF_BASE_SECS: 30`, `BULLETIN_REFRESH_CIRCUIT_OPEN_THRESHOLD: 3`, `MAINTENANCE_TASK_TIMEOUT_MIN_SECS: 300`, `MAINTENANCE_TASK_TIMEOUT_MAX_SECS: 3600`

### Process Interaction Flow
```
User message -> Channel LLM turn
  -> needs thinking -> spawns Branch (fork of history)
  -> needs execution -> spawns Worker (independent task)
  -> replies immediately via reply tool
Branch finishes -> result injected into channel history -> channel retriggered
Worker finishes -> status update injected -> channel retriggered
```

Retrigger events are debounced. Multiple branches completing within a short window batch into one turn. Limit: `MAX_RETRIGGERS_PER_TURN: 3`.

## Event System

Two `broadcast::channel<ProcessEvent>` buses per agent:
- `event_tx` — control/lifecycle (capacity: 256)
- `memory_event_tx` — memory-save telemetry for cortex (capacity: 1024)

23 event variants in `ProcessEvent` enum (defined in `src/lib.rs`):
- Channel: `BranchStarted`, `BranchResult`, `WorkerStarted`, `WorkerStatus`, `WorkerIdle`, `WorkerComplete`, `WorkerInitialResult`, `TextDelta`
- Process: `ToolStarted`, `ToolCompleted`, `StatusUpdate`
- Memory: `MemorySaved`, `CompactionTriggered`
- Communication: `AgentMessageSent`, `AgentMessageReceived`
- Tasks: `TaskUpdated`
- Workers: `WorkerPermission`, `WorkerQuestion`, `WorkerText`
- OpenCode: `OpenCodeSessionCreated`, `OpenCodePartUpdated`
- System: `CortexChatUpdate`, `SettingsUpdated`

## Type Aliases

```rust
pub type AgentId = Arc<str>;
pub type ChannelId = Arc<str>;
pub type WorkerId = uuid::Uuid;
pub type BranchId = uuid::Uuid;
pub enum ProcessType { Channel, Branch, Worker, Compactor, Cortex }
pub enum ProcessId { Channel(ChannelId), Worker(WorkerId), Branch(BranchId) }
```

## Memory System (`src/memory/`)

### Three Storage Backends
- **SQLite** (`store.rs`) — `memories` table (content, type, importance, timestamps) + `associations` table (graph edges with weights and relation types)
- **LanceDB** (`lance.rs`) — Vector embeddings (HNSW indexing) + full-text search (Tantivy). Joined on memory ID.
- **Embedding** (`embedding.rs`) — FastEmbed (local, no external API). Wrapped in `Arc<Mutex<EmbeddingModel>>` for thread safety.

### Memory Types (`src/memory/types.rs`)
```rust
pub enum MemoryType {
    Fact,        // Something true (can be updated/contradicted)
    Preference,  // User likes/dislikes
    Decision,    // A choice that was made (carries why-context)
    Identity,    // Core user/agent info (never decayed, always surfaced)
    Event,       // Something that happened (temporal, naturally decays)
    Observation, // Inferred patterns
    Goal,        // Active goals (used in bulletin)
    Todo,        // Actionable task/reminder (cortex can promote to tasks)
}
```

### Associations (`src/memory/types.rs`)
```rust
pub enum RelationType {
    RelatedTo,   // General semantic connection
    Updates,     // Newer version of same info
    Contradicts, // Conflicting information
    CausedBy,    // Causal relationship
    ResultOf,    // Result relationship
    PartOf,      // Hierarchical relationship
}
```
Auto-association: new memories search for similar ones. >0.9 similarity = `Updates` edge.

### Search Modes (`src/memory/search.rs`)
```rust
pub enum SearchMode {
    Hybrid,    // Vector (HNSW) + FTS (Tantivy) + Graph traversal, merged via RRF (default)
    Recent,    // Most recent by created_at DESC, no query needed
    Important, // Highest importance DESC, no query needed
    Typed,     // Filter by MemoryType, requires memory_type param
}

pub enum SearchSort {
    Recent,       // Most recent first (created_at DESC, default)
    Importance,   // Highest importance first
    MostAccessed, // Most accessed first (access_count DESC)
}
```
RRF (Reciprocal Rank Fusion) works on ranks not raw scores. This handles different scales of vector and keyword results without normalization.

### Memory Maintenance (cortex-managed)
- Decay: reduce importance of old, unaccessed memories (rate: 0.05/day)
- Prune: delete below importance floor (threshold: 0.1, min age: 30 days)
- Merge: combine near-duplicates (similarity threshold: 0.95)
- Identity memories exempt from decay/pruning

### Memory Bulletin
Cortex periodically generates an LLM-curated summary injected into every channel system prompt. Eight retrieval sections (programmatic), then single LLM synthesis pass (~500 words). Cached in `RuntimeConfig` via `ArcSwap`. Refresh: warmup loop (900s) + fallback (3600s).

### Working Memory (`working.rs`)
Ephemeral per-channel memory. Not persisted to SQLite/LanceDB.

## Task System (`src/tasks/`)

### Task Structure (`src/tasks/store.rs`)
```rust
pub struct Task {
    pub id: String,
    pub task_number: i64,
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub owner_agent_id: String,
    pub assigned_agent_id: String,
    pub subtasks: Vec<TaskSubtask>,
    pub metadata: Value,
    pub source_memory_id: Option<String>,
    pub worker_id: Option<String>,
    pub created_by: String,
    pub approved_at: Option<String>,
    pub approved_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

pub enum TaskStatus { PendingApproval, Backlog, Ready, InProgress, Done }
pub enum TaskPriority { Critical, High, Medium, Low }
```

### Lifecycle (from docs)
```
pending_approval -> ready (approval) or backlog (shelve)
backlog -> ready (manual promotion)
ready -> in_progress (cortex pickup)
in_progress -> done (success) or ready (failure, re-queued)
```

### Cortex Ready-Task Loop
Every `tick_interval_secs` (30s): claim oldest `ready` task -> `in_progress` -> spawn worker -> bind `worker_id` -> execute -> `done` or back to `ready`.

Workers get restricted `task_update` (subtasks/metadata only, cannot change status/priority/title/description).

## Tool System (`src/tools/`)

### Global Constants
- `MAX_TOOL_OUTPUT_BYTES: 50_000`
- `MAX_DIR_ENTRIES: 500`

### Five ToolServer Configurations
1. **Channel** — tools added/removed per turn via `add_channel_tools()`/`remove_channel_tools()`
2. **Branch** — isolated per branch, static tool set
3. **Worker** — isolated per worker, static + optional tools
4. **Cortex** — `memory_save` only
5. **Cortex Chat** — full superset including factory tools

### Tool Implementation Pattern (Rig Framework)
```rust
impl Tool for SomeTool {
    const NAME: &'static str = "tool_name";
    type Args = SomeArgs;   // Deserialize + JsonSchema
    type Error = SomeError;  // Display
    type Output = SomeOutput; // Serialize
    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error>;
}
```

Doc comments on tool input structs serve dual purpose: Rust documentation AND LLM instructions.

### Adding a New Tool
1. Create `src/tools/<name>.rs` with struct implementing `rig::tool::Tool`
2. Add `pub mod <name>;` to `src/tools.rs`
3. Add Jinja2 description template in `prompts/en/tools/<name>_description.md.j2`
4. Register in appropriate `create_*_tool_server()` factory function in `src/tools.rs`
5. If channel-dynamic, add to `add_channel_tools()`/`remove_channel_tools()`

### Tool Categories
- **Delegation:** reply, branch, spawn_worker, route, cancel, skip, react
- **Memory:** memory_save, memory_recall, memory_delete, memory_persistence_complete
- **Execution:** shell, file_read, file_write, file_edit, file_list, project_manage
- **Browser:** browser_launch, browser_navigate, browser_snapshot, browser_click, browser_type, browser_press_key, browser_screenshot, browser_evaluate, browser_tab_open, browser_tab_list, browser_tab_close, browser_close
- **Knowledge:** channel_recall, attachment_recall, wiki_*, skills_*, spacebot_docs
- **Tasks:** task_create, task_list, task_update
- **Network:** web_search, email_search, send_message_to_another_channel, send_agent_message, send_file
- **Secrets:** secret_set
- **Status:** set_status, set_outcome, worker_inspect, config_inspect
- **Scheduling:** cron
- **Factory:** factory_create_agent, factory_update_identity, factory_update_config, factory_load_preset, factory_list_presets, factory_search_context

## LLM Integration (`src/llm/`)

### Key Types
```rust
pub struct LlmManager {
    config: ArcSwap<LlmConfig>,  // Hot-reloadable
    http_client: reqwest::Client,
    rate_limited: Arc<RwLock<HashMap<String, Instant>>>,
}

pub struct SpacebotModel {
    pub name: String, pub provider: String,
    pub context_window: usize, pub max_output_tokens: usize,
}

pub struct RoutingConfig {
    channel_model, branch_model, worker_model, cortex_model: String,
}
```

### Model Name Format
`<provider>/<model>` — e.g., `anthropic/claude-sonnet-4-20250514`, `openai/gpt-4o`

### Providers (14+)
Anthropic, OpenAI, OpenRouter, Gemini, Groq, DeepSeek, XAI, Fireworks, Together, Mistral, Nvidia, Minimax, Moonshot, Azure, GitHub Copilot, custom providers

### Hot Reload
Uses `arc_swap::ArcSwap<T>` for lock-free reads. File watcher with 2s debounce re-parses config.toml and updates `RuntimeConfig` fields atomically.

Hot-reloadable: model routing, compaction thresholds, max_turns, context_window, browser config, warmup config, identity files, skills, bindings
Needs restart: LLM API keys, messaging tokens, agent topology, database paths

## Configuration (`src/config/`)

### Config File
`~/.spacebot/config.toml` or `$SPACEBOT_DIR/config.toml`

### Value Resolution
- `secret:NAME` — secret store lookup
- `env:VAR_NAME` — environment variable
- literal value
- Resolution chain: agent override > `[defaults]` > env fallback > hardcoded default

### Key Config Sections
- `[defaults]` — max_concurrent_branches (5), max_turns (5), context_window (128000), history_backfill_count (50)
- `[defaults.routing]` — channel, branch, worker, compactor, cortex model assignments + rate_limit_cooldown_secs (60)
- `[defaults.compaction]` — background_threshold (0.80), aggressive_threshold (0.85), emergency_threshold (0.95)
- `[defaults.cortex]` — tick_interval_secs (30), bulletin_interval_secs (3600), bulletin_max_words (500), worker_timeout_secs (600), branch_timeout_secs (60), maintenance_* settings
- `[defaults.warmup]` — enabled (true), eager_embedding_load (true), refresh_secs (900)
- `[defaults.browser]` — enabled (true), headless (true), evaluate_enabled (false)
- `[defaults.ingestion]` — enabled (true), poll_interval_secs (30), chunk_size (4000)
- `[defaults.opencode]` — enabled (false), max_servers (5)
- `[agents.sandbox]` — mode ("enabled"), writable_paths, passthrough_env

### Sub-modules
- `types.rs` — Config, RuntimeConfig, AgentConfig, DefaultsConfig structs
- `load.rs` — Parse config.toml + env vars + secrets
- `toml_schema.rs` — TOML structure definitions
- `runtime.rs` — In-memory state (hot-reloadable via ArcSwap)
- `permissions.rs` — Platform permission specs
- `providers.rs` — Provider default configs
- `onboarding.rs` — Interactive setup wizard
- `watcher.rs` — File watcher for hot reload

## Database Layer (`src/db.rs`)

### Triple-Database Bundle
```rust
pub struct Db {
    pub sqlite: SqlitePool,           // Per-agent relational data
    pub lance: lancedb::Connection,   // Vector embeddings
    pub redb: Arc<redb::Database>,    // Key-value config
}
```

### SQLite Tables (per-agent `spacebot.db`)
memories, associations, conversation_messages, channels, cron_jobs, cron_executions, worker_runs, branch_runs, cortex_events, cortex_chat_messages, tasks, ingestion_progress, ingestion_files, agent_profile, portal_conversations, channel_settings, token_usage, working_memory

### Global SQLite (`global.db`)
projects, project_repos, project_worktrees, notifications, wiki_pages, wiki_page_versions, wiki_pages_fts

### redb Databases
- `config.redb` — UI prefs, feature flags
- `settings.redb` — runtime settings
- `secrets.redb` — encrypted secret store (AES-256-GCM + Argon2id)

### Migration Rules
- NEVER edit existing files in `migrations/`
- Always create a new timestamped migration
- Migration files are immutable
- Format: `YYYYMMDDHHMMSS_description.sql`
- Global migrations go in `migrations/global/`

## Prompt System (`src/prompts/`)

### Template Engine
MiniJinja (Rust Jinja2). Templates embedded in binary via `include_str!`.

### Template Files (`prompts/en/`)
- `channel.md.j2`, `branch.md.j2`, `worker.md.j2`, `cortex.md.j2`, `compactor.md.j2`
- `memory_persistence.md.j2`, `ingestion.md.j2`, `cortex_bulletin.md.j2`
- `tools/*.md.j2` — 48+ tool description templates
- `fragments/` — reusable prompt fragments (worker_capabilities, conversation_context, skills_*, retrigger, truncation, etc.)

### Channel Prompt Assembly Order
1. Identity context (SOUL.md + IDENTITY.md + ROLE.md)
2. Memory bulletin (cortex-generated)
3. Base channel instructions
4. Skills prompt (if any active)
5. Worker capabilities
6. Conversation context (platform/channel metadata)
7. Status text

### PromptEngine
```rust
pub struct PromptEngine { ... }
// Key methods:
engine.render_channel_prompt(identity_context, memory_bulletin, skills_prompt, worker_capabilities, conversation_context, status_text)
engine.render_static("worker")
engine.render_worker_capabilities(browser_enabled, web_search_enabled)
```

## Identity System (`src/identity/`)

### Per-Agent Files (in `~/.spacebot/agents/{id}/`)
- `SOUL.md` — personality, values, communication style, boundaries
- `IDENTITY.md` — agent name, nature, purpose
- `ROLE.md` — responsibilities, scope, escalation rules

Live outside workspace so worker file tools cannot access them. Hot-reloaded on change.

### Presets (`presets/`)
9 agent personas: community-manager, content-writer, customer-support, engineering-assistant, executive-assistant, main-agent, project-manager, research-analyst, sales-bdr. Each has `meta.toml`, `IDENTITY.md`, `ROLE.md`, `SOUL.md`.

## Messaging (`src/messaging/`)

9 platform adapters: Discord, Slack, Telegram, Twitch, Signal, Email (IMAP/SMTP), Webhook, Mattermost, Portal (web chat).

All implement `Messaging` trait -> `MessagingManager` -> unified `InboundMessage` stream -> Channel.

Named adapter instances support multiple connections per platform (e.g., `discord:ops`).

## Security (`src/secrets/`, `src/sandbox/`)

### Sandbox
- bubblewrap (Linux) — mount namespaces, PID isolation, die-with-parent
- sandbox-exec (macOS) — SBPL deny-default
- Filesystem: read-only allowlist + writable workspace/tmp
- Agent data directory masked at kernel level

### Secret Store
Instance-level redb at `secrets.redb`. Two categories:
- System — never exposed (LLM keys, messaging tokens)
- Tool — injected as env vars to workers (GH_TOKEN, etc.)
Encryption: AES-256-GCM + Argon2id. Master key in OS credential store.

### Output Protection
1. Stream scrubbing — exact-match redaction of stored secrets (handles chunk boundaries)
2. Leak detection — regex patterns at channel egress (reply tool)
3. Exec env var blocklist — blocks LD_PRELOAD, DYLD_INSERT_LIBRARIES, NODE_OPTIONS, etc.

### Permissions (per-agent)
Five dimensions: file_read/file_write ("deny"|"workspace"|"allow"|globs), shell, exec ("deny"|"allowlist"|"allow"), browser (bool + url_allowlist), network_outbound (bool)

## API Server (`src/api/`)

Axum-based REST API on port 19898. Embedded frontend via rust-embed. OpenAPI docs via utoipa.

30 route modules: agents, channels, workers, memories, tasks, wiki, skills, config, secrets, messaging, attachments, projects, cron, notifications, links, factory, cortex, mcp, models, providers, activity, usage, ingest, system, portal, ssh, bindings, opencode_proxy, settings, tools

Special endpoints: `events_sse()` (real-time SSE), `health()`, `backup_export()`/`backup_restore()`

## Hooks System (`src/hooks/`)

- `SpacebotHook` — emits ProcessEvents for every tool call/completion, tracks memory persistence contracts, enforces cancellation, tool nudging, leak detection
- `CortexHook` — lighter cortex-specific variant
- `LoopGuard` — detects and prevents infinite loops

Hook actions: `Continue`, `Terminate`, `Skip`

## Key Constants Reference

| Constant | Value | Location |
|----------|-------|----------|
| `EVENT_SUMMARY_MAX_CHARS` | 160 | lib.rs |
| `CONTROL_EVENT_BUS_CAPACITY` | 256 | lib.rs |
| `MEMORY_EVENT_BUS_CAPACITY` | 1024 | lib.rs |
| `MAX_TOOL_OUTPUT_BYTES` | 50,000 | tools.rs |
| `MAX_DIR_ENTRIES` | 500 | tools.rs |
| `MAX_RETRIGGERS_PER_TURN` | 3 | channel_prompt.rs |
| `TURNS_PER_SEGMENT` | 15 | worker.rs |
| `MAX_OVERFLOW_RETRIES` | 2 | worker.rs, branch.rs |
| `MAX_TRANSIENT_RETRIES` | 5 | worker.rs |
| `MAX_SEGMENTS` | 10 | worker.rs |
| `DETACHED_WORKER_LIFECYCLE_TERMINAL` | 3 | process_control.rs |
| Background compaction | 0.80 | config defaults |
| Aggressive compaction | 0.85 | config defaults |
| Emergency truncation | 0.95 | config defaults |
| Context window default | 128,000 | config defaults |
| Max concurrent branches | 5 | config defaults |
| Bulletin refresh (warmup) | 900s | config defaults |
| Bulletin refresh (fallback) | 3600s | config defaults |
| Worker timeout | 600s | cortex config |
| Branch timeout | 60s | cortex config |
| Circuit breaker threshold | 3 | cortex config |
| Memory decay rate | 0.05/day | cortex config |
| Memory prune threshold | 0.1 | cortex config |
| Memory min age for pruning | 30 days | cortex config |
| Memory merge similarity | 0.95 | cortex config |
| Link conversation safety cap | 20 turns | links |

## On-Disk Structure

```
~/.spacebot/
  config.toml
  embedding_cache/
  skills/
  data/
    global.db            # instance-wide SQLite
    secrets.redb         # encrypted secret store
  agents/{id}/
    SOUL.md, IDENTITY.md, ROLE.md
    workspace/
      skills/, ingest/
    data/
      spacebot.db        # per-agent SQLite
      lancedb/           # vector search
      config.redb        # key-value settings
      settings.redb      # runtime settings
      screenshots/, logs/
    archives/            # compaction transcripts
```

## Multi-Agent Communication

Agents communicate via directed links (hierarchical/peer, one_way/two_way).

Link channels format: `link:{self}:{peer}`, safety cap of 20 turns.

Tools: `send_agent_message`, `conclude_link`.

Config: `[[links]]` with from/to agent IDs, type, direction. `[[humans]]` with id, display_name, role, bio. `[[groups]]` for visual containers.

## Cron System (`src/cron/`)

User-defined scheduled tasks. Fresh channel per execution.

Config: id, prompt, cron_expr (wall-clock), interval_secs, delivery_target, active hours, enabled, run_once, timeout_secs.

Circuit breaker: 3 consecutive failures -> auto-disable.

## CLI Commands

```
spacebot start [-f foreground] [-d debug]
spacebot stop
spacebot restart
spacebot status
spacebot secrets set <NAME>
spacebot skill add|install|list|info|remove
```

Environment: `SPACEBOT_DIR`, `SPACEBOT_DEPLOYMENT` (docker/hosted/native), `SPACEBOT_MAX_AGENTS`, `SPACEBOT_CRON_TIMEZONE`, `SPACEBOT_CHANNEL_MODEL`, `SPACEBOT_WORKER_MODEL`

## Metrics (feature-gated: `metrics`)

Prometheus-compatible at `/metrics` on port 9090. All prefixed `spacebot_`.

Key: `llm_requests_total`, `llm_request_duration_seconds`, `llm_tokens_total`, `llm_estimated_cost_dollars`, `tool_calls_total`, `tool_call_duration_seconds`, `active_workers`, `active_branches`, `memory_entry_count`, `process_errors_total`, `dispatch_while_cold_count`

## Anti-Patterns (from AGENTS.md)

These are validated constraints. Violating them produces architecturally broken code.

- **Don't block the channel.** The channel never waits on branches, workers, or compaction. If the channel awaits a branch result before responding, the design is wrong.
- **Don't dump raw search results into channel context.** Memory recall goes through a branch, which curates. The channel gets clean conclusions.
- **Don't give workers channel context.** Workers get a fresh prompt and a task. If it needs conversation context, that's a branch.
- **Don't make the compactor an LLM process.** The compactor is programmatic. The LLM work happens in the compaction worker it spawns.
- **Don't use `#[async_trait]`.** Use native RPITIT. Only add a `Dyn` companion trait when you actually need `dyn Trait`.
- **Don't create many small files.** Implement in existing files unless it's a new logical component.
- **Don't add features without updating existing docs.** Update relevant docs in the same commit. Don't create new doc files for this.

## Patterns to Implement (from AGENTS.md)

These are validated patterns. Implement them when building the relevant module.

- **Tool nudging / outcome gate:** Workers cannot exit with text-only response until they signal a terminal outcome via `set_status(kind: "outcome")`. Hook fires `Terminate` and retries with nudge prompt (up to 2 retries). See `docs/design-docs/tool-nudging.md`.
- **Fire-and-forget DB writes:** `tokio::spawn` for history saves, memory writes, log persistence. User gets response immediately.
- **Hybrid search with RRF:** `score = sum(1/(60 + rank))`. RRF works on ranks, not raw scores.
- **Leak detection:** Regex patterns for API keys, tokens, PEM keys. Scan in `SpacebotHook.on_tool_result()` and before outbound HTTP.
- **Workspace path guard:** File tools reject writes to identity/memory paths with error directing LLM to the correct tool.
- **Circuit breaker:** Auto-disable recurring tasks after 3 consecutive failures. Apply to cron, maintenance, cortex routines.
- **Error-as-result for tools:** Tool errors are structured results, not panics. The LLM sees errors and can recover.
- **Worker state machine:** Validate transitions with `can_transition_to()` using `matches!`. Illegal transitions are runtime errors.

## Build Order (6 phases from AGENTS.md)

When implementing from scratch, follow this order:

1. **Foundation** — `error.rs`, `config.rs`, `db/`, `llm/`, `main.rs`
2. **Memory** — `memory/types.rs`, `store.rs`, `lance.rs`, `embedding.rs`, `search.rs`, `maintenance.rs`
3. **Agent Core** — `hooks/spacebot.rs`, `agent/status.rs`, `tools/`, `agent/worker.rs`, `branch.rs`, `channel.rs`, `compactor.rs`
4. **System** — `identity/`, `conversation/`, `prompts/`, `agent/cortex.rs`, `hooks/cortex.rs`, `cron/`
5. **Messaging** — `messaging/traits.rs`, `manager.rs`, `webhook.rs`, `telegram.rs`, `discord.rs`
6. **Hardening** — `secrets/`, `settings/`, leak detection, workspace path guards, circuit breakers

## Frontend Architecture (from SPACEUI_MIGRATION.md)

### Component Library
Local UI primitives replaced by SpaceUI packages:
- `@spacedrive/primitives` — base components
- `@spacedrive/ai` — AI-specific components
- `@spacedrive/forms` — form components
- `@spacedrive/explorer` — file explorer components

Tailwind v4 via `@tailwindcss/vite` with SpaceUI design token system.

### Layout
Persistent 220px sidebar with accordion agent sub-nav. Global workers popover in footer. Dashboard landing page with notifications, token usage, and activity cards.

### Key UI Patterns
- **Settings** — decomposed from 2900-line monolith into 12 section components
- **AgentConfig** — decomposed from 1452 lines into ConfigSidebar + section editors
- **TopologyGraph** — decomposed from 2074 lines into OrgGraph + ProfileNode + GroupNode + edge/config panels
- **Tasks** — Linear-style task list (replaced kanban board), detail views, SSE-driven updates
- **Portal** — modular PortalPanel/Timeline/Composer/Header, file attachments, multipart upload

### Backend Features Supporting UI
- `ConversationSettings` struct with memory mode, delegation, worker context, model selection
- `ResponseMode` enum: `Active`, `Observe`, `MentionOnly` (replaces old `listen_only_mode`)
- Per-channel persistence via `ChannelSettingsStore` with resolution chain: per-channel DB > binding defaults > agent defaults
- Settings hot-reload via `ProcessEvent::SettingsUpdated`
- "Direct mode" gives channels full worker-level tools
- `UsageAccumulator` for per-process token tracking

## Metrics Instrumentation (from METRICS.md)

All telemetry behind `#[cfg(feature = "metrics")]`. Zero runtime cost without the feature. Docker always includes metrics.

### Key Metrics (all prefixed `spacebot_`)
- **LLM:** `llm_requests_total`, `llm_request_duration_seconds`, `llm_tokens_total{direction}`, `llm_estimated_cost_dollars`
- **Tools:** `tool_calls_total{tool_name}`, `tool_call_duration_seconds`
- **MCP:** `mcp_connections`, `mcp_tools_registered`, `mcp_tool_calls_total`, `mcp_tool_call_duration_seconds`
- **Messaging:** `messages_received_total`, `messages_sent_total`, `message_handling_duration_seconds`, `channel_errors_total`
- **Memory:** `memory_reads_total`, `memory_writes_total`, `memory_updates_total{operation}`, `memory_entry_count`, `memory_operation_duration_seconds`, `memory_search_results`, `memory_embedding_duration_seconds`
- **Workers:** `active_workers`, `active_branches`, `branches_spawned_total`, `worker_duration_seconds`, `worker_cost_dollars`, `context_overflow_total`
- **Cost:** `llm_estimated_cost_dollars`
- **API:** `http_requests_total`, `http_request_duration_seconds`
- **Cron:** `cron_executions_total`
- **Warmup:** `dispatch_while_cold_count`, `warmup_recovery_latency_ms`

Total cardinality: ~800-9200 series (safe for any Prometheus deployment).

Uses private `prometheus::Registry` (not global) to avoid conflicts. Metrics server on `0.0.0.0:9090`, separate from API server on `127.0.0.1:19898`.

### Adding New Metrics
1. Add metric definition in `src/telemetry/registry.rs`
2. Gate instrumentation with `#[cfg(feature = "metrics")]` at statement level
3. Access registry via `crate::telemetry::Metrics::global()`
4. Update `METRICS.md` with new metric documentation

## Development Workflow

### Before any push
```bash
just gate-pr    # Full validation pipeline
```

### Quick checks
```bash
cargo fmt --all                    # Format
cargo clippy --all-targets         # Lint
cargo test --lib                   # Unit tests
cargo test --tests --no-run        # Compile integration tests
just check-typegen                 # Verify TypeScript schema sync
```

**Cadence guidance:** See `.claude/rules/rust-iteration-loop.md` for which of these to run per change class. The inner loop should use the narrowest check that catches the bug class. Reserve `just gate-pr` for pre-push.

**Coding discipline:** See `.claude/rules/coding-discipline.md`. Surface assumptions before implementing, keep changes surgical, and default to TDD with the named escape hatches for docs, config, and async-state paths.

### Frontend
Always use `bun`, never npm/pnpm/yarn:
```bash
cd interface && bun install && bun run dev    # Dev server at :19840
cd docs && bun install && bun run dev         # Docs site
```

### Common Patterns When Implementing Features

**Adding a new process capability:**
1. Modify the appropriate `src/agent/*.rs` process file
2. If adding a tool, follow the tool addition steps above
3. Update the prompt template in `prompts/en/` if the LLM needs to know about it
4. Add ProcessEvent variant if new lifecycle events needed

**Modifying the memory system:**
1. Changes to `src/memory/store.rs` for SQLite operations
2. Changes to `src/memory/lance.rs` for vector/embedding operations
3. New migration in `migrations/` if schema changes needed
4. Update search in `src/memory/search.rs` if query patterns change

**Adding a messaging adapter:**
1. Create `src/messaging/<platform>.rs` implementing `Messaging` trait
2. Add to `MessagingManager` in `src/messaging/manager.rs`
3. Add config section in `src/config/toml_schema.rs`

**Config changes:**
1. Add TOML field in `src/config/toml_schema.rs`
2. Add to `Config` struct in `src/config/types.rs`
3. If hot-reloadable, add `ArcSwap` field to `RuntimeConfig` in `src/config/runtime.rs`
4. Parse in `src/config/load.rs`

**API endpoint:**
1. Create route handler in `src/api/<resource>.rs`
2. Register in router in `src/api/server.rs`
3. Add OpenAPI annotations via utoipa macros
4. Run `just typegen` to regenerate TypeScript types
