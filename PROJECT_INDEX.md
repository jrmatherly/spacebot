# Spacebot Project Index

**Version:** 0.4.1 | **Edition:** 2024 | **LOC:** ~130K Rust | **License:** FSL-1.1-ALv2

A Rust single-binary agentic system with process-level concurrency, structured memory persistence, and multi-channel team orchestration.

---

## Project Structure

```
spacebot/
├── src/                           (206 .rs files)
│   ├── agent/                     (15 files) - Channel, worker, branch, cortex orchestration
│   ├── api/                       (32 files) - REST endpoints (axum + utoipa OpenAPI)
│   ├── config/                    (8 files)  - TOML loading, permissions, provider routing
│   ├── llm/                       (7 files)  - Rig-core orchestration, model routing, pricing
│   ├── memory/                    (7 files)  - Graph store, working memory, search, maintenance
│   ├── messaging/                 (12 files) - Discord, Slack, Telegram, Twitch, Email, Cron
│   ├── tools/                     (48 files) - LLM-callable tools (multiple per file in some cases)
│   ├── conversation/              (8 files)  - Channel history, settings, context, portal
│   ├── tasks/                     (2 files)  - Goal/task state machine
│   ├── skills/                    (2 files)  - Skill installation, bundling, discovery
│   ├── opencode/                  (3 files)  - Worker transcript UI embedding
│   ├── db.rs                      - SQLite + LanceDB + redb initialization
│   ├── cron.rs, config.rs, factory.rs, error.rs
│   ├── prompts/, identity/, secrets/, settings/, hooks/, telemetry/
│   └── main.rs, lib.rs            - CLI daemon + module exports
├── interface/                      (Vite + React + TypeScript)
│   ├── src/components/            - 60+ React components
│   ├── src/api/                   - OpenAPI client (code-gen from Rust spec)
│   └── package.json               - React 19, Tailwind 4, React Router
├── docs/                           (40 .mdx files, Fumadocs + Next.js)
├── desktop/                        (Tauri 2 app)
├── migrations/                     (42 SQL migrations, 2026-02 → 2026-04)
├── presets/                        (9 agent persona presets)
├── prompts/                        (86 Jinja2 system prompt templates)
├── scripts/                        (7 shell scripts)
├── vendor/                         (imap-proto vendored crate)
├── spacedrive/                     (vendored Spacedrive platform, ~50MB, independent Cargo workspace, own toolchain `stable`)
└── tests/                          (11 integration test files)
```

---

## Entry Points

| Entry Point | Purpose |
|---|---|
| **src/main.rs** | CLI daemon (start/stop/restart/status, skill, auth) |
| **src/lib.rs** | Library root with 33 public modules |
| **src/bin/openapi_spec.rs** | Generates OpenAPI 3.0 JSON from Rust types |
| **src/bin/cargo-bump.rs** | Version bumper tool |

---

## Core Modules

| Module | Purpose |
|---|---|
| **agent** | Channel, worker, branch, cortex, compactor process model |
| **api** | 32 REST endpoints via axum + utoipa OpenAPI |
| **config** | TOML loader, permissions, provider routing, runtime watcher |
| **llm** | Rig-core v0.35 orchestration, model routing, pricing, auth |
| **memory** | Graph store (typed SQLite), working memory, semantic search |
| **messaging** | Discord, Slack, Telegram, Twitch, Email adapters |
| **conversation** | Channel state, history, participants, portal |
| **tools** | 50+ LLM-callable functions (browser, git, file, web, Docker) |
| **tasks** | Goal/task state machine with worker delegation |
| **skills** | Skill bundling, installation, discovery |
| **cron** | Background job scheduler |
| **factory** | Agent preset system (9 personas) |
| **mcp** | Model Context Protocol client (rmcp v1.1) |
| **secrets** | AES-256-GCM encryption, argon2 key derivation |
| **hooks** | Pre/post-channel hooks, loop guards, cortex events |
| **telemetry** | Prometheus metrics (feature-gated) |

---

## Configuration Files

| File | Purpose |
|---|---|
| **Cargo.toml** | Rust deps, features (metrics), patch directives |
| **rust-toolchain.toml** | Rust 1.85+ version pin |
| **Dockerfile** | Multi-stage build (Rust + Node) |
| **fly.toml** | Fly.io production deployment |
| **justfile** | Task runner recipes (gate-pr, preflight, typegen) |
| **interface/package.json** | Frontend deps (bun managed) |
| **docs/package.json** | Docs site deps (Next.js + Fumadocs) |

---

## Key Dependencies

| Category | Dependencies |
|---|---|
| **Runtime** | tokio 1.44 |
| **LLM** | rig-core 0.35 |
| **HTTP** | axum 0.8, reqwest 0.13 |
| **Databases** | sqlx 0.8 (SQLite), lancedb 0.27, redb 4.0 |
| **Embeddings** | fastembed 5 |
| **Discord** | serenity (git next branch) |
| **Slack** | slack-morphism 2.19 |
| **Telegram** | teloxide 0.17 |
| **Browser** | chromiumoxide 0.9 |
| **MCP** | rmcp 1.1 |
| **Templates** | minijinja 2.8 |
| **OpenAPI** | utoipa 5, utoipa-axum 0.2 |

---

## Quick Start

```bash
git clone https://github.com/jrmatherly/spacebot
cd spacebot
cargo build --release
./target/release/spacebot start --foreground
./target/release/spacebot auth login

# Frontend dev
cd interface && bun install && bun run dev

# Docs dev
cd docs && bun install && bun run dev

# CI gate
just gate-pr
```

---

## Test Coverage

- 823 `#[test]` + `#[tokio::test]` annotations across src/ (graph reports 203 Test nodes)
- 11 dedicated integration test files in tests/
- CI gate: `just gate-pr` enforces fmt + clippy + tests + migration safety

---

## Agent Presets

Nine persona presets under `presets/` — each with `IDENTITY.md`, `ROLE.md`, `SOUL.md`, `meta.toml`.

| Preset | Role |
|---|---|
| **main-agent** | Default generalist |
| **community-manager** | Community engagement |
| **content-writer** | Editorial authoring |
| **customer-support** | Support triage |
| **engineering-assistant** | Technical pairing |
| **executive-assistant** | Scheduling + comms |
| **project-manager** | Task orchestration |
| **research-analyst** | Research synthesis |
| **sales-bdr** | Outbound prospecting |

---

## Design Docs

`docs/design-docs/` — 47 architecture and implementation notes. Partial index (see directory for full list):

| Domain | Docs |
|---|---|
| **Agent model** | agent-factory, autonomy, branch-and-spawn, cortex-{chat,history,implementation}, worker-briefing, workers-tab |
| **Memory** | working-memory, working-memory-implementation-plan, working-memory-problem-analysis, working-memory-triage, tiered-memory, user-scoped-memories |
| **Tasks** | task-tracking, goals, global-task-elevation, link-channels-task-delegation |
| **Messaging** | named-messaging-adapters, multi-agent-communication-graph, participant-awareness, channel-attachment-persistence, channel-settings-unification, conversation-settings, attachment-portal-and-defaults |
| **Cron** | cron-outcome-delivery, cron-timezone-and-reliability |
| **Sandbox** | sandbox, sandbox-hardening, interactive-shell |
| **Integrations** | mcp, stereos-integration, slash-commands, skill-authoring, projects |
| **Secrets & security** | secret-store, sandbox-hardening |
| **Observability** | live-logs, token-usage-tracking, production-worker-failures |
| **Frontend** | openapi-migration, api-client-package-followup, wiki |
| **Prompts** | prompt-routing, tool-nudging |

---

## Project Rules (`.claude/rules/`)

Ten rule files that govern agent behavior across Rust edits, messaging parity, API handler conventions, tool authoring, and writing style.

| Rule | Scope |
|---|---|
| **rust-essentials.md** | Core Rust conventions (imports, naming, errors, lints) |
| **rust-patterns.md** | Subsystem patterns (async, Rig, Serde, state machines) |
| **rust-iteration-loop.md** | Fast inner-loop tool selection (fmt → check → clippy → tests) |
| **coding-discipline.md** | Behavioral guardrails (surface assumptions, simplicity, surgical edits, goal-driven TDD) |
| **async-state-safety.md** | Race conditions, cancellation, terminal-state reasoning |
| **messaging-adapter-parity.md** | Cross-adapter feature consistency |
| **provider-integration.md** | LLM provider wiring and pricing |
| **api-handler.md** | Axum handler conventions (path-scoped: `src/api/**`) |
| **tool-authoring.md** | Rig tool definition conventions (path-scoped: `src/tools/**`) |
| **writing-guide.md** | Copy voice and anti-patterns |

---

## OpenSpec Changes

Under `openspec/changes/` — structured change proposals with specs + phased tasks.

No active changes at present. Recently archived:

| Archived change | Summary |
|---|---|
| `2026-04-16-security-remediation-obsolete` | Security remediation workstream (superseded / complete) |
| `2026-04-16-spacebot-dependency-remediation` | Dependency advisory remediation |
| `2026-04-16-integrate-spacedrive` | Vendor Spacedrive into `spacedrive/` in preparation for HTTP integration |
| `2026-04-15-integrate-spaceui` | Adopt SpaceUI (`spaceui/`) as the frontend design system |
| `2026-04-15-fix-rustls-webpki-audit` | Serenity `next` branch pin to resolve rustls-webpki advisories |
| `2026-04-15-upgrade-dependencies` | Workspace-level dependency wave |
| `2026-04-15-dependency-upgrades` | Frontend dependency wave (TypeScript 6, HeadlessUI 2, Vite 8, Storybook 10) |

---

## Documentation

| File | Topic |
|---|---|
| README.md | Project overview, quick start, deploy |
| CHANGELOG.md | Version history (upstream attribution through v0.4.1) |
| CONTRIBUTING.md | Dev workflow, PR gates, SpaceUI linking |
| AGENTS.md | Architecture implementation guide |
| METRICS.md | Prometheus metrics reference |
| RUST_STYLE_GUIDE.md | Coding conventions |
| SPACEUI_MIGRATION.md | Frontend migration changelog |
| CLAUDE.md | AI assistant context |
| docs/design-docs/ | 47 architecture + implementation notes |
| openspec/ | Active change proposals + archived specs |
