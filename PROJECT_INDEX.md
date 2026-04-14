# Project Index: Spacebot

Generated: 2026-04-14

## Overview

**Spacebot** (`v0.4.1`) is a Rust agentic system where every LLM process has a dedicated role. It runs as a daemon with an HTTP API, manages multiple AI agents via configurable presets, and provides tools for memory, tasks, wiki, messaging, projects, and more. Deployed on Fly.io.

- **Language**: Rust (2024 edition), ~130K lines
- **Database**: SQLite via sqlx (42 migrations)
- **Vector DB**: LanceDB for memory embeddings
- **Runtime**: Tokio async
- **API**: Axum HTTP framework
- **Deployment**: Docker → Fly.io (region: iad)

## Project Structure

```
spacebot/
├── src/              # Rust source (206 files)
│   ├── main.rs       # CLI entry point (clap)
│   ├── lib.rs        # Library root — 34 public modules
│   ├── bin/          # Extra binaries (openapi-spec, cargo-bump)
│   ├── agent/        # Agent lifecycle & orchestration
│   ├── api/          # Axum HTTP routes
│   ├── config/       # TOML config loading, runtime, permissions
│   ├── conversation/ # Conversation state management
│   ├── factory/      # Agent factory (create, presets, identity)
│   ├── hooks/        # Event hooks system
│   ├── identity/     # Agent identity files
│   ├── llm/          # LLM providers, routing, pricing, Anthropic
│   ├── memory/       # Vector memory (LanceDB, embeddings, search)
│   ├── messaging/    # Inter-process messaging
│   ├── opencode/     # OpenCode protocol (SSE streaming)
│   ├── projects/     # Project management, git integration
│   ├── sandbox/      # Tool sandboxing
│   ├── secrets/      # Keystore, secret scrubbing
│   ├── skills/       # Skill installation & management
│   ├── tasks/        # Task store & migration
│   ├── telemetry/    # Prometheus metrics (feature-gated)
│   ├── tools/        # 48 LLM tools (see below)
│   └── wiki/         # Wiki CRUD & search
├── interface/        # Web UI (Vite + React + TypeScript)
│   ├── src/          # React app, components, routes
│   └── opencode-embed-src/  # Embeddable widget variant
├── docs/             # Documentation site (Next.js + Fumadocs)
├── desktop/          # Tauri desktop app (spacebot-desktop)
├── migrations/       # 42 SQLite migrations (2026-02 → 2026-04)
├── presets/          # 9 agent persona presets
├── scripts/          # Build & release scripts
├── tests/            # 11 integration test files
├── vendor/           # Vendored crate: imap-proto-0.10.2
├── nix/              # Nix build support
├── flake.nix         # Nix flake definition
└── justfile          # Task runner recipes
```

## Entry Points

- **CLI/Daemon**: `src/main.rs` → binary `spacebot`
- **OpenAPI Spec**: `src/bin/openapi_spec.rs` → binary `openapi-spec`
- **Version Bump**: `src/bin/cargo-bump.rs`
- **Web UI**: `interface/src/main.tsx`
- **Docs Site**: `docs/app/`
- **Desktop**: `desktop/src-tauri/`

## CLI Subcommands

| Command | Description |
|---------|-------------|
| `start` | Start the daemon (foreground option) |
| `stop` | Stop the running daemon |
| `restart` | Stop + start |
| `status` | Show daemon status |
| `skill` | Manage skills (add, install, list) |
| `auth` | Manage authentication (login, logout, refresh) |
| `secrets` | Manage secrets in running instance |

## Core Modules (src/)

| Module | Purpose |
|--------|---------|
| `agent` | Agent lifecycle, orchestration, event loops |
| `api` | Axum HTTP router, REST endpoints |
| `auth` | Authentication layer |
| `config` | TOML config, runtime settings, permissions, onboarding |
| `conversation` | Conversation state & history |
| `cron` | Scheduled job execution |
| `daemon` | Daemonization, PID management |
| `db` | SQLite database setup & migrations |
| `factory` | Agent creation from presets, identity management |
| `hooks` | Event hook system |
| `identity` | Agent identity file management |
| `llm` | LLM manager, model routing, pricing, Anthropic provider |
| `memory` | Vector memory: LanceDB store, embeddings, search, maintenance |
| `messaging` | Inter-process message bus |
| `notifications` | Notification delivery |
| `opencode` | OpenCode protocol, SSE streaming |
| `projects` | Project management, git integration |
| `prompts` | System prompt construction |
| `sandbox` | Tool execution sandboxing |
| `secrets` | Keystore (macOS Keychain), secret scrubbing |
| `self_awareness` | Agent self-awareness context |
| `settings` | Settings store |
| `skills` | Skill installation & registry |
| `tasks` | Task CRUD & migration |
| `telemetry` | Prometheus metrics (feature: `metrics`) |
| `tools` | 48 LLM-callable tools |
| `update` | Self-update mechanism |
| `wiki` | Wiki pages CRUD & search |

## LLM Tools (48)

attachment_recall, branch_tool, browser, cancel, channel_recall, config_inspect, cron, email_search, factory_create_agent, factory_list_presets, factory_load_preset, factory_search_context, factory_update_config, factory_update_identity, file, install_skill, mcp, memory_delete, memory_persistence_complete, memory_recall, memory_save, project_manage, react, read_skill, reply, route, secret_set, send_agent_message, send_file, send_message_to_another_channel, set_outcome, set_status, shell, skills_search, skip, spacebot_docs, spawn_worker, task_create, task_list, task_update, web_search, wiki_create, wiki_edit, wiki_history, wiki_list, wiki_read, wiki_search, worker_inspect

## Agent Presets (9)

community-manager, content-writer, customer-support, engineering-assistant, executive-assistant, main-agent, project-manager, research-analyst, sales-bdr

Each preset contains: `IDENTITY.md`, `ROLE.md`, `SOUL.md`, `meta.toml`

## Process Types

The system runs 5 process types: **Channel**, **Branch**, **Worker**, **Compactor**, **Cortex**

## Key Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| tokio | 1.44 | Async runtime |
| axum | 0.8 | HTTP framework |
| sqlx | 0.8 | SQLite (async, migrations) |
| lancedb | 0.26 | Vector database for memory |
| reqwest | 0.13 | HTTP client |
| clap | 4.5 | CLI argument parsing |
| serde/serde_json | 1.0 | Serialization |
| tracing | 0.1 | Structured logging |
| uuid | 1.15 | Unique identifiers |
| tokio-tungstenite | 0.28 | WebSocket support |
| security-framework | 3 | macOS Keychain (macOS only) |
| prometheus | — | Metrics (feature-gated) |

## Configuration

- `Cargo.toml` — Rust package & dependencies
- `fly.toml` / `fly.staging.toml` — Fly.io deployment (port 19898)
- `flake.nix` — Nix build environment
- `justfile` — Task runner (build, test, release recipes)
- `Dockerfile` — Container build
- `build.rs` — Build script

## Documentation

- `README.md` — Project overview & setup
- `AGENTS.md` — AI agent coding guidelines
- `CONTRIBUTING.md` — Contribution guide
- `CHANGELOG.md` — Release history
- `RUST_STYLE_GUIDE.md` — Rust coding standards
- `METRICS.md` — Metrics documentation
- `SPACEUI_MIGRATION.md` — UI migration plan
- `docs/` — Full documentation site (Next.js)

## Tests

- **Integration tests**: 11 files in `tests/`
  - context_dump, maintenance, detached_worker_bootstrap
  - cron_integration_test, bulletin
  - sandbox_detection_test, sandbox_initialization_test, tool_sandbox_integration_test
  - tool_nudge, opencode_stream, opencode_sse

## Quick Start

1. `nix develop` or install Rust toolchain manually
2. `just build` — Build the project
3. `just run` — Start the daemon
4. `just test` — Run tests

## Scripts

| Script | Purpose |
|--------|---------|
| `build-opencode-embed.sh` | Build embeddable UI widget |
| `bundle-docs.sh` | Bundle documentation site |
| `bundle-sidecar.sh` | Bundle sidecar assets |
| `gate-pr.sh` | PR gate checks |
| `install-git-hooks.sh` | Set up git hooks |
| `preflight.sh` | Pre-build validation |
| `release-tag.sh` | Create release tags |
