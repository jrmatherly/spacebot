# Spacebot - Project Structure

Single binary crate. No workspace, no sub-crates.

```
spacebot/
├── src/                  # 206 Rust source files
│   ├── main.rs           # CLI entry point (clap subcommands: start, stop, restart, status, skill, auth, secrets)
│   ├── lib.rs            # Library root — 34 public modules, shared types
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
│   ├── tasks/            # Task CRUD & migration
│   ├── telemetry/        # Prometheus metrics (feature-gated)
│   ├── tools/            # 48 LLM-callable tool files (63 tool implementations)
│   └── wiki/             # Wiki pages CRUD & search
├── interface/            # Web UI (Vite + React + TypeScript)
│   ├── src/              # React app, components, routes, hooks
│   └── opencode-embed-src/  # Embeddable widget variant
├── spaceui/              # SpaceUI design system (6 packages: tokens, primitives, forms, icons, ai, explorer)
├── docs/                 # Documentation site (Next.js + Fumadocs)
├── desktop/              # Tauri desktop app (spacebot-desktop)
├── migrations/           # 42 SQLite migrations: agent + global (2026-02 → 2026-04)
├── presets/              # 9 agent persona presets (each has IDENTITY.md, ROLE.md, SOUL.md, meta.toml)
├── scripts/              # Build & release scripts
├── tests/                # 11 integration test files
├── vendor/               # Vendored crate: imap-proto-0.10.2
├── nix/                  # Nix build support
├── flake.nix             # Nix flake definition
└── justfile              # Task runner recipes
```

## Module File Convention
Never create `mod.rs` files. Use `src/memory.rs` as the module root (not `src/memory/mod.rs`).
The module root file contains `mod` declarations and re-exports.
