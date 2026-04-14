# Spacebot

@RUST_STYLE_GUIDE.md

## Quick Start

```bash
nix develop              # Enter dev shell (or install Rust toolchain manually)
cargo build              # Build the project
cargo run -- start       # Start the daemon (port 19898)
```

## Build & Test

- Run `just gate-pr` before any push or PR
- Run `just preflight` to validate git/remote/auth state
- If the same command fails twice, stop and debug root cause
- Do not push when any gate is red
- `cargo test --lib` for unit tests
- `cargo test --tests --no-run` to compile integration tests
- `cargo fmt --all` to format, `cargo clippy --all-targets` to lint

## Architecture

Five process types, each a Rig `Agent<SpacebotModel, SpacebotHook>`:

1. **Channel** -- User-facing conversation. Delegates everything. Never blocked.
2. **Branch** -- Fork of channel context for independent thinking. Short-lived.
3. **Worker** -- Background task executor. Has shell, file, and memory tools.
4. **Compactor** -- Context management and summarization.
5. **Cortex** -- System-level intelligence.

Single binary crate. No workspace, no sub-crates. Module files use `src/module.rs` pattern (NEVER `src/module/mod.rs`).

## Package Managers

- Rust: `cargo`
- Frontend (`interface/`): `bun` (NEVER npm/pnpm/yarn)
- Desktop (`desktop/`): `cargo tauri`

## Database Migrations

- NEVER edit existing files in `migrations/`
- Always create a new timestamped migration for schema changes
- Treat migration files as immutable

## Lints

- `dbg_macro`, `todo`, `unimplemented` are `deny` in `[lints.clippy]`
- Never discard errors with `let _ =`
- Use `.ok()` only on channel sends where receiver may be dropped
- Use `.context()` for adding context to errors

## Imports

Grouped into 3 tiers separated by blank lines, alphabetical within each:

1. Crate-local (`use crate::...`)
2. External crates (alphabetical by crate name)
3. Standard library (`use std::...`)

Suppress unused trait warnings: `use anyhow::Context as _;`

## Comments

- Explain WHY, never WHAT
- Module-level `//!` doc comment at top of every file
- `///` on public APIs and constants
- `// TODO:` for tracked future work (never `todo!()` macro)
- No organizational/section-divider comments
- No alarmist language (`CRITICAL:`, `IMPORTANT FIX:`)

## Key Directories

- `prompts/` — Jinja2 system prompt templates (channel, branch, worker, cortex)
- `presets/` — Agent persona presets (IDENTITY.md, ROLE.md, SOUL.md, meta.toml)
- `migrations/` — 42 SQLite migrations (immutable, append-only)
- `vendor/` — Vendored crates (imap-proto)
- `interface/` — Web UI (Vite + React + TypeScript)
- `docs/` — Documentation site (Next.js + Fumadocs)
- `desktop/` — Tauri desktop app

## Frontend

Always use `bun`, never npm/pnpm/yarn:

| Command | Purpose |
|---------|---------|
| `bun install` | Install dependencies |
| `bun run dev` | Start dev server |
| `bun run build` | Production build |
| `bun run test` | Run tests |

If TypeScript types changed: `just check-typegen` to verify schema sync.
