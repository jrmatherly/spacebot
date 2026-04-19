# Contributing

Contributions welcome. Read [RUST_STYLE_GUIDE.md](RUST_STYLE_GUIDE.md) before writing any code, and [AGENTS.md](AGENTS.md) for the full implementation guide.

---

## Prerequisites

- **Rust** 1.94.1 with `rustfmt` and `clippy` (pinned in `rust-toolchain.toml`; rustup installs the right channel automatically)
- **protoc** (protobuf compiler)
- **bun** (for frontend/interface work)
- **just** (`brew install just` or `cargo install just --locked`)

Optional: [Nix flakes](https://nixos.org/) for isolated dev environments (`nix develop` gives you everything).

---

## Getting Started

1. Fork the repo and create a feature branch
2. Run `./scripts/install-git-hooks.sh` (installs a pre-commit hook that runs `cargo fmt`)
3. `cargo build` to verify the backend compiles
4. For frontend work: `cd interface && bun install`
5. Make your changes
6. Run `just preflight && just gate-pr`
7. Submit a PR

---

## PR Gate

Every PR must pass `just gate-pr` before merge. This mirrors CI and checks:

1. **Sidecar naming** — Tauri sidecar binary name agrees across every reference site
2. **Formatting** — `cargo fmt --all -- --check`
3. **Lints** — `cargo clippy --all-targets -Dwarnings` (strict superset of `cargo check`)
4. **Tests** — `cargo test --lib`
5. **Integration compile** — `cargo test --tests --no-run`

Use `just gate-pr-fast` for tight iteration. Fast mode substitutes `cargo check` for clippy and skips the integration-test compile; run full `just gate-pr` before pushing.

The frontend CI (`interface-ci.yml`) runs `bun ci` and `bunx tsc --noEmit` on interface changes.

---

## Local Build Tuning

The repo ships an optimised `[profile.dev]` in `Cargo.toml` that cuts debug binary size from ~555 MB to ~170 MB and shrinks `target/debug/incremental/` by roughly half. The defaults favour iteration speed over debugger ergonomics. Two escape hatches are available when you need the full experience:

- `just debug-build` — builds with `CARGO_PROFILE_DEV_DEBUG=2` for complete variable and type inspection in `lldb`/`rust-gdb`.
- `just sweep-target` — runs `cargo sweep` to prune stale toolchain artifacts. For a deeper reset use `just clean-all` (wipes Rust + frontend build state).

### Editor setup

Copy `.vscode/settings.json.example` to `.vscode/settings.json` (the target file is gitignored so each contributor can customise). The example enables `rust-analyzer.cargo.targetDir = true`, which puts rust-analyzer's build artifacts in `target/rust-analyzer/` so it stops invalidating the CLI's incremental cache on every save. For Helix, Neovim, or Zed the equivalent setting is `rust-analyzer.cargo.target_dir`.

### Frontend iteration

The Rust build no longer re-runs the frontend build on TypeScript source edits — `build.rs` only watches `interface/`'s config files (`package.json`, `bun.lock`, `index.html`, `vite.config.ts`, `tailwind.config.ts`). When iterating on frontend code, run the Vite dev server (`cd interface && bun run dev` at `:19840`) so changes hot-reload independently of the daemon. Before testing the embedded UI served by `cargo run`, run `just check-frontend` (or `cd interface && bun run build` directly). Release CI keeps the full frontend build.

### macOS tuning (one-time, per-developer)

Two macOS system settings deliver a significant speedup and neither touches the repo:

1. **Exclude `target/` from Spotlight indexing.** System Settings → Spotlight → Search Privacy → drag in your dev directory, or run `sudo mdutil -i off target/`.
2. **Allow your terminal past XProtect scanning.** System Settings → Privacy & Security → Developer Tools → add Terminal.app / iTerm2 / WezTerm / Ghostty. Nethercote measured rustc test-suite runtime drop from 9m42s to 3m33s with this alone.
3. **Confirm Homebrew deps are arm64-native** (skip if you're on an Intel Mac): `file $(brew --prefix)/lib/*.dylib | grep -v arm64`. Any x86_64 hits are running under Rosetta and will slow compilation.

### Linux tuning

`nix develop` already wires `mold` via `nix/default.nix` — Nix users get fast linking automatically. Bare-metal Linux contributors can opt in manually: install `mold` and `clang` (package names vary per distro), then uncomment the `[target.x86_64-unknown-linux-gnu]` or `[target.aarch64-unknown-linux-gnu]` block in `.cargo/config.toml`. Verify with `which mold clang` first.

---

## Project Structure

Single binary crate with no workspace **members**. The root `Cargo.toml` carries `[workspace] exclude = ["spacedrive"]` to keep the vendored Spacedrive workspace out of Cargo auto-discovery. Key directories:

```
src/
├── main.rs           — CLI entry, config, startup
├── lib.rs            — re-exports
├── config.rs         — config loading/validation
├── error.rs          — top-level Error enum
├── llm/              — LlmManager, model routing, providers
├── agent/            — Channel, Branch, Worker, Compactor, Cortex
├── hooks/            — SpacebotHook, CortexHook
├── tools/            — reply, branch, spawn_worker, memory_*, etc.
├── memory/           — MemoryStore, hybrid search, graph ops
├── messaging/        — Discord, Telegram, Slack, webhook adapters
├── conversation/     — history persistence, context assembly
├── cron/             — scheduler, CRUD
├── identity/         — SOUL.md, IDENTITY.md, USER.md loading
├── secrets/          — encrypted credentials (AES-256-GCM)
├── settings/         — key-value settings
└── db/               — SQLite migrations, connection setup

interface/            — Dashboard UI (Vite + React + TypeScript)
prompts/              — LLM prompts as markdown (not Rust strings)
docs/                 — Documentation site (MDX)
desktop/              — Tauri desktop app
scripts/              — Dev tooling (hooks, gates, builds)
```

Module roots use `src/module.rs`, **not** `src/module/mod.rs`.

---

## Rust Conventions

The full guide is in [RUST_STYLE_GUIDE.md](RUST_STYLE_GUIDE.md). Key points:

**Imports** — three tiers separated by blank lines: (1) crate-local, (2) external crates, (3) std.

**Error handling** — domain errors per module, wrapped by top-level `Error` enum via `#[from]`. Use `?` and `.context()`. Never silently discard with `let _ =`.

**Async** — native RPITIT for async traits (not `#[async_trait]`). `tokio::spawn` for concurrent work. Clone before moving into async blocks.

**Logging** — `tracing` crate, never `println!`. Structured key-value fields. `#[tracing::instrument]` for spans.

**Lints** (enforced in Cargo.toml): `dbg_macro = "forbid"`, `todo = "forbid"`, `unimplemented = "forbid"`.

**Testing** — `#[cfg(test)]` at end of file. `#[tokio::test]` for async. `.unwrap()` is fine in tests only.

---

## Frontend (Interface)

Use **bun** exclusively — never npm, pnpm, or yarn.

```bash
cd interface
bun install       # install deps
bun run dev       # dev server
bun run build     # production build
```

### SpaceUI Packages

The dashboard uses `@spacedrive/*` packages from the `spaceui/` directory at the project root:

- `@spacedrive/primitives` — base UI components (Radix, CVA, framer-motion)
- `@spacedrive/ai` — AI chat components (ToolCall, ChatComposer, TaskBoard)
- `@spacedrive/forms` — form field wrappers (react-hook-form)
- `@spacedrive/explorer` — file explorer components (FileThumb, GridItem)
- `@spacedrive/tokens` — CSS design tokens and 7 theme variants

The interface resolves these from TypeScript source via Vite aliases (`interface/vite.config.ts`). No `bun link` is needed for development. Start both dev servers:

```bash
# Terminal 1: SpaceUI watch mode (rebuilds on change)
cd spaceui && bun install && bun run dev

# Terminal 2: Interface dev server
cd interface && bun run dev
```

Changes to SpaceUI source files trigger HMR in the interface automatically.

---

## Spacedrive

Spacedrive lives at `spacedrive/` as an independent Cargo workspace. The two projects share zero Rust crates and are vendored together so the planned HTTP integration can be developed from a single clone. Runtime coupling does not exist yet; the in-tree copy is preparation.

**Always `cd spacedrive` before running cargo commands inside it.** The root `cargo` invocations only see Spacebot. Spacedrive declares its own `[workspace]` and Spacebot's `Cargo.toml` carries `[workspace] exclude = ["spacedrive"]` to prevent auto-discovery.

**Toolchain.** Spacedrive pins `channel = "stable"` in `spacedrive/rust-toolchain.toml`. Spacebot pins `1.94.1` at the root. Rustup resolves the right toolchain per directory, so no manual switching is needed as long as you `cd` first.

**Bun workspaces.** Each TypeScript-bearing directory is its own Bun workspace with its own `bun.lock`: `interface/`, `spaceui/`, `docs/`, and `spacedrive/`. `cd` into the target directory before running `bun install`, `bun run dev`, etc.

**Formatter.** Spacedrive's `.rustfmt.toml` sets `hard_tabs = true`. The root `cargo fmt --all` only touches Spacebot's source (different workspace). Run `cd spacedrive && cargo fmt --all` separately when editing Spacedrive Rust files.

**Build artifacts.** `spacedrive/target/` and the various `node_modules/` and `.next/` directories are gitignored. The entire `spacedrive/` directory is in `.dockerignore` because Spacebot's Docker image does not need it.

---

## Useful Commands

```bash
just preflight                # validate git/remote state
just gate-pr                  # full PR gate (mirrors CI)
just gate-pr --fast           # skip clippy + integration compile
just typegen                  # generate TypeScript API types
just check-typegen            # verify types match
just build-opencode-embed     # build OpenCode embed bundle
just bundle-sidecar           # build Tauri sidecar
just desktop-dev              # run desktop app in dev mode
just update-frontend-hash     # update Nix hash after frontend dep changes
```

---

## Migrations

SQLite migrations are **immutable**. Never edit an existing migration file. Always create a new timestamped migration for schema changes.

---

## Security and Dependency Policy

Dependabot security alerts in spacebot-owned code that are blocked on upstream crate or package updates are tracked in-repo at [`docs/security/deferred-advisories.md`](docs/security/deferred-advisories.md). The file documents each open advisory, why it is deferred, which upstream we are waiting on, and the trigger that would unblock local resolution.

Review `docs/security/deferred-advisories.md` whenever dependencies are refreshed. Do not dismiss deferred advisories via the GitHub API — visibility on the dashboard is intentional.

Spacedrive-scoped Dependabot alerts (under `spacedrive/**`) are left open and will be re-triaged when the planned runtime integration lands. Any OpenSpec change that proposes spacebot↔spacedrive runtime coupling must include a task to re-evaluate those alerts.

---

## Architecture

See the [Architecture](<docs/content/docs/(core)/architecture.mdx>) page for the full design. The short version: five process types, each with one job.

- **Channels** — user-facing LLM, stays responsive, never blocks on work
- **Branches** — fork channel context to think, return conclusion, get deleted
- **Workers** — independent task execution with focused tools, no conversation context
- **Compactor** — programmatic context monitor, triggers compaction before channels fill up
- **Cortex** — system observer, generates memory bulletins, supervises processes

Key rule: **never block the channel**. Branch to think, spawn workers to act.

---

## Release Process

Releases are triggered by git tags (`v*`). The CI workflow:

1. Verifies `Cargo.toml` version matches the tag
2. Builds multi-platform binaries (x86_64/aarch64, Linux/macOS)
3. Builds Docker images (amd64/arm64)
4. Creates a GitHub release with binaries
5. Updates the Homebrew tap

---

## License

FSL-1.1-ALv2 ([Functional Source License](https://fsl.software/)), converting to Apache 2.0 after two years.
