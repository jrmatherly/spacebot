# Spacebot

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
- `cargo audit --ignore RUSTSEC-2023-0071` for security audit

## Architecture

Single binary crate with no workspace **members**. The root `Cargo.toml` carries `[workspace] exclude = ["spacedrive"]` — an intentional guard that prevents Cargo from auto-discovering the vendored `spacedrive/` workspace. Do not delete the `[workspace]` block; if anything, the only safe change is to extend the exclude list. Module files use `src/module.rs` pattern (NEVER `src/module/mod.rs`). Five process types (Channel, Branch, Worker, Compactor, Cortex), each a Rig `Agent<SpacebotModel, SpacebotHook>`. Three databases: SQLite (relational), LanceDB (vectors), redb (key-value).

## Package Managers

- Rust: `cargo`
- Frontend (`interface/`): `bun` (NEVER npm/pnpm/yarn)
- SpaceUI (`spaceui/`): `bun` (independent workspace, own bun.lock)
- Desktop (`desktop/`): `cargo tauri`

## Database Migrations

- NEVER edit existing files in `migrations/`
- Always create a new timestamped migration for schema changes
- Treat migration files as immutable

## Key Directories

- `prompts/` — Jinja2 system prompt templates
- `presets/` — Agent persona presets (IDENTITY.md, ROLE.md, SOUL.md, meta.toml)
- `migrations/` — SQLite migrations (immutable, append-only)
- `vendor/` — Vendored crates (imap-proto)
- `interface/` — Web UI (Vite + React + TypeScript)
- `spaceui/` — SpaceUI design system (6 packages: tokens, primitives, forms, icons, ai, explorer)
- `spacedrive/` — Spacedrive platform (independent Cargo workspace, own toolchain). Always `cd spacedrive` before running cargo commands inside it. Vendored in preparation for the planned HTTP integration; no live runtime coupling exists yet.
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

## Reference Docs

- `RUST_STYLE_GUIDE.md` — Full Rust coding conventions
- `AGENTS.md` — Architecture implementation guide for coding agents
- `METRICS.md` — Prometheus metrics reference
- `SPACEUI_MIGRATION.md` — Frontend migration changelog
- `PROJECT_INDEX.md` — Module index and dependency map
- `CONTRIBUTING.md` — Contributor guide
- `CHANGELOG.md` — Release history
