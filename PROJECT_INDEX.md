# Spacebot Project Index

**Version:** 0.5.1 | **Edition:** 2024 | **LOC:** ~130K Rust | **License:** FSL-1.1-ALv2

A Rust single-binary agentic system with process-level concurrency, structured memory persistence, and multi-channel team orchestration.

---

## Project Structure

```
spacebot/
├── src/                           (215 .rs files)
│   ├── agent/                     (15 files) - Channel, worker, branch, cortex orchestration
│   ├── api/                       (32 files) - REST endpoints (axum + utoipa OpenAPI)
│   ├── config/                    (8 files)  - TOML loading, permissions, provider routing
│   ├── llm/                       (7 files)  - Rig-core orchestration, model routing, pricing
│   ├── memory/                    (7 files)  - Graph store, working memory, search, maintenance
│   ├── messaging/                 (12 files) - Discord, Slack, Telegram, Twitch, Email, Cron
│   ├── tools/                     (49 files) - LLM-callable tools (multiple per file in some cases)
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
│   └── package.json               - React 19, Tailwind 4, React Router
├── packages/                       (Workspace packages under the @spacebot/* scope)
│   └── api-client/                - OpenAPI TypeScript client (code-gen from Rust spec; consumed by interface/)
├── docs/                           (38 .mdx files, Fumadocs + Next.js)
├── desktop/                        (Tauri 2 app)
├── migrations/                     (48 SQL migrations: 41 flat per-agent + 7 instance-wide under global/, 2026-02 → 2026-04)
├── presets/                        (11 agent persona presets)
├── prompts/                        (91 Jinja2 system prompt templates)
├── scripts/                        (10 active shell scripts + scripts/_disabled/check-migration-safety.sh)
├── vendor/                         (imap-proto vendored crate)
├── spacedrive/                     (vendored Spacedrive platform, ~50MB, independent Cargo workspace, own toolchain `stable`)
└── tests/                          (16 integration test files)
```

---

## Entry Points

| Entry Point | Purpose |
|---|---|
| **src/main.rs** | CLI daemon (start/stop/restart/status, skill, auth) |
| **src/lib.rs** | Library root with 36 public modules |
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
| **tools** | 49 LLM-callable tool files (browser, git, file, web, Docker) |
| **tasks** | Goal/task state machine with worker delegation |
| **skills** | Skill bundling, installation, discovery |
| **cron** | Background job scheduler |
| **factory** | Agent preset system (11 personas) |
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
| **deploy/docker/** | Docker Compose variant (one file, six profiles: default, build, spacedrive, proxy, observability, tooling) |
| **deploy/helm/** | Kubernetes Helm chart (bjw-s-labs/app-template wrapper for Talos) |
| **justfile** | Task runner recipes (gate-pr, preflight, typegen, compose-*) |
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

- 885 `#[test]` + `#[tokio::test]` annotations across src/
- 15 dedicated integration test files in tests/
- CI gate: `just gate-pr` enforces check-sidecar-naming + 3 frontend invariant guards (check-workspace-protocol, check-vite-dedupe, check-adr-anchors) + fmt + clippy (RUSTFLAGS=-Dwarnings) + lib tests + integration test compile. Migration-safety check is defined but disabled; the enforcement logic lives at `scripts/_disabled/check-migration-safety.sh` and can be reactivated from there. Use `just gate-pr-fast` for tight iteration (cargo check in place of clippy, skip integration compile; does NOT propagate -Dwarnings).

---

## Agent Presets

Eleven persona presets under `presets/` — each with `IDENTITY.md`, `ROLE.md`, `SOUL.md`, `meta.toml`.

| Preset | Role |
|---|---|
| **main-agent** | Default generalist |
| **community-manager** | Community engagement |
| **content-writer** | Editorial authoring |
| **customer-support** | Support triage |
| **engineering-assistant** | Technical pairing |
| **executive-assistant** | Scheduling + comms |
| **integration-engineer** | Third-party API + webhook + MCP wiring |
| **project-manager** | Task orchestration |
| **research-analyst** | Research synthesis |
| **sales-bdr** | Outbound prospecting |
| **sre** | Incident response + on-call + postmortems |

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
| **Frontend** | openapi-migration, frontend-api-client, wiki |
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

Active (implemented, awaiting archive):

| Active change | Summary |
|---|---|
| `integrate-spacedrive-track-a-config` | Track A Phase 1 — `[spacedrive]` config section, `SpacedriveIntegrationConfig` shape |
| `integrate-spacedrive-track-a-client` | Track A Phase 2 — outbound HTTP client with `{"Query":...}` envelope, HTTPS enforcement, 10 MB response cap |
| `integrate-spacedrive-track-a-tool-list-files` | Track A Phase 3 — `spacedrive_list_files` agent tool, prompt-injection envelope, pairing migration, secrets integration |

Recently archived:

| Archived change | Summary |
|---|---|
| `2026-04-16-security-remediation-obsolete` | Security remediation workstream (superseded / complete) |
| `2026-04-16-spacebot-dependency-remediation` | Dependency advisory remediation |
| `2026-04-16-integrate-spacedrive` | Vendor Spacedrive into `spacedrive/` as an independent Cargo workspace (prerequisite for Track A) |
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
| docs/design-docs/spaceui-migration.md | Frontend migration changelog |
| CLAUDE.md | AI assistant context |
| docs/design-docs/ | 55 architecture + implementation notes (1 archived under `docs/design-docs/archive/`) |
| openspec/ | Active change proposals + archived specs |
