# Spacebot - Development Commands

## Task Runner (just)

| Command | Purpose |
|---------|---------|
| `just` | List all available recipes |
| `just preflight` | Validate git/remote/auth state before pushing |
| `just gate-pr` | Full PR gate: formatting, clippy (supersets check), unit tests, integration compile. `cargo check` was dropped from the full gate 2026-04-19 (Sprint 1 local-build-optimization); migration-safety check disabled 2026-04-16. |
| `just gate-pr-fast` | Fast local gate — runs cargo check (no clippy) + unit tests. For tight iteration loops; run full `just gate-pr` before pushing. Added 2026-04-19. |
| `just gate-pr-nextest` | Same gates as `just gate-pr` but unit-test step uses `cargo nextest run --lib`. Equivalent to `./scripts/gate-pr.sh --nextest` or `GATE_PR_NEXTEST=1 just gate-pr`. Added 2026-04-20. |
| `just check-fast` | Narrowest useful inner-loop check: `cargo clippy --lib --no-deps`. Added 2026-04-19. |
| `just check-frontend` | Rebuild embedded frontend UI (`cd interface && bun run build`). Needed when iterating on `interface/src/` TypeScript because build.rs no longer watches that path. Added 2026-04-19. |
| `just debug-build` | Build with full debug symbols (`CARGO_PROFILE_DEV_DEBUG=2`) for lldb/rust-gdb variable inspection. Default dev profile uses `line-tables-only`. Added 2026-04-19. |
| `just sweep-target` | Prune stale cargo artifacts (requires `cargo install cargo-sweep`). Added 2026-04-19. |
| `just clean-all` | Nuclear cleanup — wipes target/, interface/dist, interface/node_modules, spaceui/node_modules, spaceui/packages/*/dist, opencode-embed, .fastembed_cache. Added 2026-04-19. |
| `just clean-frontend` | Lighter cleanup — frontend only (interface + spaceui), keeps Rust target/. Added 2026-04-19. |
| `just fmt-check` | Check Rust formatting (`cargo fmt --all -- --check`) |
| `just check-all` | `cargo check --all-targets` |
| `just clippy-all` | `cargo clippy --all-targets` |
| `just test-lib` | Run library unit tests (`cargo test --lib`) |
| `just test-lib-nextest` | Run library unit tests via cargo-nextest (process-per-test isolation, parallel scheduling). Requires `cargo install cargo-nextest`. Added 2026-04-20 (PR after #78). |
| `just test-integration-compile` | Compile integration tests without running |
| `just typegen` | Generate TypeScript types from OpenAPI spec. Writes to `packages/api-client/src/schema.d.ts` (retargeted from `interface/src/api/schema.d.ts` in PR #75, 2026-04-19) |
| `just check-typegen` | Verify TypeScript types are up-to-date. Also enforced in CI at `.github/workflows/ci.yml` `check-typegen` job (added PR #75); fails the PR if regen produces a diff. |
| `just build-opencode-embed` | Build embeddable UI widget |
| `just bundle-sidecar` | Build binary and copy to Tauri sidecar |
| `just desktop-dev` | Run desktop app in dev mode |
| `just desktop-build` | Full desktop app build |
| `just update-frontend-hash` | Update Nix hash after frontend dep changes |
| `just update-flake` | Update all Nix flake inputs |
| `just spaceui-build` | Build SpaceUI packages (turbo-cached; run before `tsc --noEmit` in interface/) |
| `just spaceui-link` | Retired stub. `interface/package.json` declares spaceui as workspaces; `bun install` in interface/ now creates the symlinks directly |
| `just spaceui-unlink` | Retired stub. Workspace protocol does not need unlinking |
| `just spaceui-check-workspace` | Run the workspace-protocol guard over every package.json (PR #52) |
| `just spaceui-check-dedupe` | Audit vite dedupe list against shared spaceui/interface deps (PR #52) |
| `just spaceui-gate` | Typecheck + build spaceui, then typecheck + build interface; includes both checks above (PR #52) |
| `just check-adr-anchors` | Verify path:line anchors in Spacedrive integration ADRs still resolve (PR #53) |

## Docker Compose Recipes (deploy/docker/)

| Command | Purpose |
|---------|---------|
| `just compose-up` | Start Spacebot via published image (default profile) |
| `just compose-up-build` | Rebuild Spacebot from root Dockerfile (build profile; mutually exclusive with default) |
| `just compose-up-spacedrive` | Spacebot + in-tree Spacedrive integration harness |
| `just compose-up-observability` | Default + Prometheus + Grafana + Grafana Alloy (OTLP collector on 4317/gRPC + 4318/HTTP) stack |
| `just compose-up-litellm` | Spacebot + LiteLLM proxy sidecar (requires LITELLM_MASTER_KEY in .env). Added 2026-04-20 in PR #78 alongside the `[providers.litellm]` config block. |
| `just compose-up-all` | Full stack: default + spacedrive + proxy + observability + tooling |
| `just compose-down` | Stop all services across all profiles (Compose v2.20+) |
| `just compose-down-compat` | Fallback down for Compose < 2.20 |
| `just compose-reset` | DESTRUCTIVE: stop + wipe all named volumes (typed WIPE confirmation) |
| `just compose-logs` | Tail logs across all running services |
| `just compose-proxy-trust` | Install Caddy's local CA into host trust store |
| `just compose-proxy-untrust` | Remove Caddy's local CA from host trust store |
| `just compose-validate` | Validate compose config for every profile (CI mirror) |

## Delivery Gates (Mandatory before push/PR)
1. `just preflight` — validate git/remote/auth state
2. `just gate-pr` — formatting, compile, clippy, unit tests, integration compile

## Rust Commands
| Command | Purpose |
|---------|---------|
| `cargo build` | Build the project |
| `cargo run` | Run the daemon |
| `cargo test` | Run all tests |
| `cargo fmt --all` | Format code |
| `cargo clippy --all-targets` | Lint code |
| `cargo audit --ignore RUSTSEC-2023-0071` | Security audit |

## Frontend Commands (interface/)
**Always use `bun`, never npm/pnpm/yarn**

| Command | Purpose |
|---------|---------|
| `bun install` | Install dependencies |
| `bun run dev` | Start dev server |
| `bun run build` | Production build |
| `bun run test` | Run tests |
| `bunx <tool>` | Run npx-equivalent |

## System Utilities (macOS/Darwin)
| Command | Purpose |
|---------|---------|
| `git` | Version control |
| `ls` / `find` / `grep` | File system navigation |
| `nix develop` | Enter Nix development shell |
| `nix build .#frontend` | Build frontend via Nix |
