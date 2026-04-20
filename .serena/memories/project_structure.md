# Spacebot - Project Structure

Single binary crate with no workspace **members**. The root `Cargo.toml` carries `[workspace] exclude = ["spacedrive"]` to keep the vendored Spacedrive workspace out of Cargo auto-discovery.

```
spacebot/
├── src/                  # 215 Rust source files
│   ├── main.rs           # CLI entry point (clap subcommands: start, stop, restart, status, skill, auth, secrets)
│   ├── lib.rs            # Library root — 35 public modules, shared types
│   ├── bin/              # Extra binaries: openapi-spec, cargo-bump
│   ├── agent/            # Agent lifecycle & orchestration (channel, branch, worker, compactor, cortex)
│   ├── api/              # Axum HTTP router & REST endpoints
│   ├── config/           # TOML config: runtime, types, load, permissions, onboarding, watcher, providers
│   ├── conversation/     # Conversation state management
│   ├── factory/          # Agent creation from presets, identity management
│   ├── hooks/            # Event hooks system
│   ├── identity/         # Agent identity file management
│   ├── llm/              # LLM manager, model routing, pricing, Anthropic provider (auth, cache, tools, params)
│   ├── memory/           # Vector memory: LanceDB store, embeddings, search, maintenance, working memory
│   ├── messaging/        # Inter-process message bus
│   ├── opencode/         # OpenCode protocol, SSE streaming
│   ├── projects/         # Project management, git integration
│   ├── sandbox/          # Tool execution sandboxing
│   ├── secrets/          # Keystore (macOS Keychain), secret scrubbing
│   ├── skills/           # Skill installation & registry
│   ├── spacedrive/       # Spacedrive integration (Track A complete): config, HTTP client with `{"Query":...}` envelope + HTTPS enforcement, error taxonomy, wire types including SdPath, prompt-injection envelope, build_client_from_secrets helper. Runtime-gated via `enabled` flag
│   ├── tasks/            # Task CRUD & migration
│   ├── telemetry/        # Prometheus metrics (feature-gated)
│   ├── tools/            # 49 LLM-callable tool files (64 tool implementations; `spacedrive_list_files` added 2026-04-17)
│   └── wiki/             # Wiki pages CRUD & search
├── interface/            # Web UI (Vite + React + TypeScript). No local src/api/ anymore — consumes `@spacebot/api-client` from packages/ via workspace symlink (PR #75, 2026-04-19).
│   ├── src/              # React app, components, routes, hooks
│   └── opencode-embed-src/  # Embeddable widget variant
├── packages/             # @spacebot/* workspace scope. Currently: api-client/ (TypeScript client for Spacebot REST API + SSE event types; codegen target for `just typegen`). Added 2026-04-19 in PR #75 as the activate-api-client-package OpenSpec change. Subpath-only exports (no root barrel): ./client, ./types, ./schema. Any future sibling here must publish under @spacebot/* to be covered by scripts/check-workspace-protocol.sh.
├── spaceui/              # SpaceUI design system (6 packages: tokens, primitives, forms, icons, ai, explorer). Consumed by interface/ via bun workspace protocol — interface/package.json declares "workspaces": ["../spaceui/packages/*", "../packages/*"] (symlinks both @spacedrive/* and @spacebot/*) and pins each workspace dep to "workspace:*"
├── spacedrive/           # Vendored Spacedrive platform (independent Cargo workspace, own toolchain `stable`). Now a real fork — PR #57 authored 10 stub files under core/src/ to unblock sd-server compile. SYNC.md LOCAL_STATE register is load-bearing; the fork is ours per the 2026-04-16 self-reliance decision (ownership reframe in commits 778d4c3 + 48f536b). Never bulk-rsync from any external source.
├── docs/                 # Documentation site (Next.js + Fumadocs)
├── desktop/              # Tauri desktop app (spacebot-desktop)
├── migrations/           # 48 SQLite migrations: 41 flat per-agent + 7 instance-wide under global/ (2026-02 → 2026-04)
├── presets/              # 11 agent persona presets (each has IDENTITY.md, ROLE.md, SOUL.md, meta.toml; integration-engineer + sre added 2026-04-19 in PR #74)
├── scripts/              # 10 build & release scripts + scripts/_disabled/check-migration-safety.sh (relocated 2026-04-20, not wired into any gate). gate-pr.sh wires 4 guards: check-sidecar-naming, check-workspace-protocol, check-vite-dedupe, check-adr-anchors.
├── tests/                # 13 integration test files (`spacedrive_client.rs` with wiremock-backed RPC envelope + 401/Bearer round-trip tests; `litellm_proxy.rs` added 2026-04-19 in PR #77 covering [llm.providers.<id>] table form + [[providers]] top-level array form for LLM base_url routing; extended 2026-04-20 in PR #78 with `from_openai_body_parses_litellm_anthropic_cache_tokens` regression guard for `prompt_tokens_details.cached_tokens` parsing)
├── vendor/               # Vendored crate: imap-proto-0.10.2
├── nix/                  # Nix build support
├── flake.nix             # Nix flake definition
└── justfile              # Task runner recipes
```

## Module File Convention
Never create `mod.rs` files. Use `src/memory.rs` as the module root (not `src/memory/mod.rs`).
The module root file contains `mod` declarations and re-exports.
