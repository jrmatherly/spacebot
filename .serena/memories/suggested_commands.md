# Spacebot - Development Commands

## Task Runner (just)

| Command | Purpose |
|---------|---------|
| `just` | List all available recipes |
| `just preflight` | Validate git/remote/auth state before pushing |
| `just gate-pr` | Full PR gate: formatting, compile, clippy, unit tests, integration compile (migration-safety check disabled 2026-04-16; see CLAUDE.md Database Migrations section) |
| `just fmt-check` | Check Rust formatting (`cargo fmt --all -- --check`) |
| `just check-all` | `cargo check --all-targets` |
| `just clippy-all` | `cargo clippy --all-targets` |
| `just test-lib` | Run library unit tests (`cargo test --lib`) |
| `just test-integration-compile` | Compile integration tests without running |
| `just typegen` | Generate TypeScript types from OpenAPI spec |
| `just check-typegen` | Verify TypeScript types are up-to-date |
| `just build-opencode-embed` | Build embeddable UI widget |
| `just bundle-sidecar` | Build binary and copy to Tauri sidecar |
| `just desktop-dev` | Run desktop app in dev mode |
| `just desktop-build` | Full desktop app build |
| `just update-frontend-hash` | Update Nix hash after frontend dep changes |
| `just update-flake` | Update all Nix flake inputs |
| `just spaceui-build` | Build SpaceUI packages |
| `just spaceui-link` | Link SpaceUI packages for development |
| `just spaceui-unlink` | Unlink SpaceUI, restore npm versions |

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
