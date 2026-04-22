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
- `cargo nextest run --lib` (or `just test-lib-nextest`) for unit tests; `cargo test --lib` kept for debugging shared-state tests. `just gate-pr` defaults to nextest as of the R1 flip on 2026-04-22.
- `cargo test --tests --no-run` to compile integration tests (also useful as TDD red-pass: skips execution on known-red tests)
- `cargo fmt --all` to format, `cargo clippy --all-targets` to lint (clippy supersets check; never run both)
- `cargo audit --ignore RUSTSEC-2023-0071` for security audit
- `just check-typegen` mandatory after any `src/api/**/*.rs` edit (utoipa annotations regenerate `packages/api-client/src/schema.d.ts`)
- If a unit test hangs for >60s on code that spawns background tasks (OTLP, LanceDB indexers, etc.), it is the current-thread-runtime deadlock — use `#[tokio::test(flavor = "multi_thread")]`. See `.claude/skills/test-runtime-patterns/SKILL.md`.

## Architecture

Single binary crate with no workspace **members**. The root `Cargo.toml` carries `[workspace] exclude = ["spacedrive"]` — an intentional guard that prevents Cargo from auto-discovering the vendored `spacedrive/` workspace. Do not delete the `[workspace]` block; if anything, the only safe change is to extend the exclude list. Module files use `src/module.rs` pattern (NEVER `src/module/mod.rs`). Five process types (Channel, Branch, Worker, Compactor, Cortex), each a Rig `Agent<SpacebotModel, SpacebotHook>`. Three databases: SQLite (relational), LanceDB (vectors), redb (key-value).

## Package Managers

- Rust: `cargo`
- Frontend (`interface/`): `bun` (NEVER npm/pnpm/yarn)
- SpaceUI (`spaceui/`): `bun` (its own workspace + bun.lock; `interface/` declares `"workspaces": ["../spaceui/packages/*", "../packages/*"]` so `bun install` in interface symlinks both `@spacedrive/*` and `@spacebot/*` to local source). The `../packages/*` glob is intentionally permissive: any future sibling under `packages/` is auto-adopted as a workspace member. Sibling packages MUST publish under the `@spacebot/*` scope so the workspace-protocol guard protects them. **Never remove the `workspaces` declaration or change a `workspace:*` dep to a semver range** — bun will silently fall back to the public npm registry and overwrite local customizations. The `scripts/check-workspace-protocol.sh` guard runs on every `interface/` preinstall, in CI (`.github/workflows/spaceui.yml`), and as part of `just gate-pr` to catch this class of regression (covers both `@spacedrive/*` and `@spacebot/*` scopes). The guard uses `git ls-files` for sub-200ms enforcement; it requires a git worktree and fails loudly in contexts where `.git` is absent (e.g., Docker build stages). See `spaceui/SYNC.md` for the full provenance and drift discipline.
- Desktop (`desktop/`): `cargo tauri`

## Database Migrations

- Prefer creating a new timestamped migration for schema changes
- Historical migration files may be edited for formatting or clarity, but be aware: SQLx stores migration checksums in `_sqlx_migrations` at apply time, so editing an already-applied migration will cause startup to fail on that database until the stored checksum is repaired or the DB is reset
- Keep SQL semantics unchanged when reformatting historical migrations

## Key Directories

- `prompts/` — Jinja2 system prompt templates
- `presets/` — Agent persona presets (IDENTITY.md, ROLE.md, SOUL.md, meta.toml)
- `migrations/` — SQLite migrations (append-only by default; reformatting allowed with checksum-repair awareness)
- `vendor/` — Vendored crates (imap-proto)
- `interface/` — Web UI (Vite + React + TypeScript). Consumes `@spacebot/api-client` and `@spacedrive/*` packages via workspace symlink.
- `packages/` — Internal `@spacebot/*` workspace packages. Currently: `api-client/` (TypeScript client for the Spacebot REST API + SSE event types; codegen target for `just typegen`).
- `spaceui/` — SpaceUI design system (6 packages: tokens, primitives, forms, icons, ai, explorer)
- `spacedrive/` — Spacebot-owned fork of the Spacedrive platform (independent Cargo workspace, own toolchain). Always `cd spacedrive` before running cargo commands inside it. Runtime integration lives at `src/spacedrive/` (config, HTTP client, envelope, first agent tool `spacedrive_list_files`). Track A complete on main; runtime-gated behind `[spacedrive] enabled = true`. As of PR #57 and the 2026-04-16 self-reliance decision, this is a genuine Spacebot fork: 10 Spacebot-authored stub files under `spacedrive/core/src/` plus an `apps/web/dist/index.html` placeholder unblock the `sd-server` build. `spacedrive/SYNC.md` LOCAL_STATE register is load-bearing — never bulk-rsync from any external source.
- `docs/` — Documentation site (Next.js + Fumadocs)
- `desktop/` — Tauri desktop app
- `deploy/docker/` — Docker Compose variant (one file, six profiles: default, build, spacedrive, proxy, observability, tooling). See `deploy/docker/README.md` and `just compose-*` recipes.
- `deploy/helm/` — Kubernetes Helm chart (production deployment target on Talos)

## Frontend

Always use `bun`, never npm/pnpm/yarn:

| Command | Purpose |
|---------|---------|
| `bun install` | Install dependencies |
| `bun run dev` | Start dev server |
| `bun run build` | Production build |
| `bun run test` | Run tests |

If TypeScript types changed: `just check-typegen` to verify schema sync.

`packages/api-client/src/schema.d.ts` is **generated** and hook-blocked from hand edits. Modify `src/api/**/*.rs` utoipa annotations, then run `just typegen`.

## Graphify (opt-in)

Graphify is a local knowledge-graph tool installed per-developer via `pipx install graphifyy`. It is not wired into any CI, hook, or automatic workflow. Use it when a semantic cross-document question would benefit from Leiden clustering over design docs, RFCs, and screenshots. For structural Rust code questions, use the existing `code-review-graph` MCP instead.

Three entry points, all opt-in:

```
just graphify-rebuild docs/design-docs/   # build directed graph for design docs
just graphify-query "<question>"          # query the built graph
just graphify-clean                       # drop graphify-out/ entirely
```

`.graphifyignore` at the repo root governs what gets ingested. Do not remove `node_modules/` or `spacedrive/` from that list — they were excluded deliberately (cost sizing + noise-to-signal ratio during the 2026-04-21 evaluation).

## Reference Docs

- `RUST_STYLE_GUIDE.md` — Full Rust coding conventions
- `.claude/rules/coding-discipline.md` — Surface assumptions, simplicity, surgical edits, goal-driven TDD
- `docs/design-docs/spacedrive-integration-pairing.md` — Shared-state contract between Spacebot and Spacedrive (blocks Track A and Track B)
- `docs/design-docs/spacedrive-tool-response-envelope.md` — Prompt-injection defense envelope for Spacedrive-returned tool bytes
- `docs/design-docs/entra-app-registrations.md` — Entra app-registration schema (client IDs, app roles, redirect URIs, API permissions) used by the Phase 1 JWT validator
- `docs/design-docs/entra-backfill-strategy.md` — No-auto-broadening policy for pre-existing resources under Entra auth, plus the Phase 10 sweep design for both orphan directions
- `docs/design-docs/entra-audit-log.md` — Phase 5 operator guide: `audit_events` table shape, chain-verification procedure, three export modes (Filesystem dev-only, S3 COMPLIANCE, HttpSiem), separation-of-duties (SOC 2 CC6.6)
- `spacedrive/SYNC.md` — Fork provenance + reference-clone discipline for the vendored Spacedrive tree (Spacebot-owned per 2026-04-16 self-reliance decision)
- `spaceui/SYNC.md` — Cherry-pick discipline for the vendored SpaceUI tree
- `AGENTS.md` — Architecture implementation guide for coding agents
- `METRICS.md` — Prometheus metrics reference
- `docs/design-docs/spaceui-migration.md` — Frontend migration changelog
- `docs/design-docs/preset-authoring.md` — Guide for adding new agent presets (4-file structure, meta.toml schema, voice rules)
- `docs/design-docs/agent-factory.md` — Factory flow, preset system, and the Prompt Authoring section covering the Jinja2 prompt tree
- `PROJECT_INDEX.md` — Module index and dependency map
- `CONTRIBUTING.md` — Contributor guide
- `CHANGELOG.md` — Release history
