# Spacebot - Project Overview

**Version:** 0.4.1
**Language:** Rust (edition 2024), ~130K lines of code
**Purpose:** An agentic system where every LLM process has a dedicated role. Replaces the monolithic session model with specialized processes (Channel, Branch, Worker, Compactor, Cortex).

## Tech Stack
- **Runtime:** Tokio async
- **HTTP Framework:** Axum 0.8
- **Database:** SQLite via sqlx 0.8 (48 migrations total: 42 in flat per-agent `migrations/`, 6 in instance-wide `migrations/global/` including `spacedrive_pairing`, 2026-02 → 2026-04)
- **Vector DB:** LanceDB 0.27 (embedded vector + FTS for memory)
- **Key-Value Store:** redb 4.0 (embedded)
- **LLM Framework:** Rig v0.35 (agentic loop framework)
- **CLI:** clap 4.5
- **Serialization:** serde/serde_json 1.0
- **Logging:** tracing 0.1
- **Error Handling:** thiserror + anyhow
- **Discord:** Serenity (git `next` branch — pulls rustls 0.23 + tokio-tungstenite 0.28 to resolve rustls-webpki audit advisories)
- **WebSocket:** tokio-tungstenite 0.28
- **Metrics:** Prometheus (feature-gated behind `metrics` feature)
- **macOS Keychain:** security-framework 3

## Frontend
- **Web UI:** Vite + React + TypeScript (`interface/`)
- **JS Package Manager:** bun (NEVER npm/pnpm/yarn)
- **Documentation Site:** Next.js + Fumadocs (`docs/`)
- **SpaceUI:** Design system (6 packages: tokens, primitives, forms, icons, ai, explorer) (`spaceui/`)
- **Spacedrive:** Vendored upstream platform at `spacedrive/` — independent Cargo workspace with its own toolchain (`stable`), excluded via `[workspace] exclude = ["spacedrive"]` in root `Cargo.toml`. Track A complete on main as of 2026-04-17: Phase 1 config (PR #54), Phase 2 HTTP client with `{"Query":...}` envelope + HTTPS enforcement + wiremock tests (PR #55), Phase 3 first agent tool `spacedrive_list_files` + prompt-injection envelope + pairing migration + secrets integration (PR #56). Runtime-gated behind `enabled`. Post-merge smoke test (Task 18) blocked on the vendored fork not compiling — 9 declared-but-unwritten modules in sd-core. Scope captured at `.scratchpad/2026-04-17-spacedrive-fork-stub-writing.md` for a future session.
- **Desktop App:** Tauri (`desktop/`)

## Security

- Dependabot alerts in spacebot-owned code that are blocked on upstream crate updates are tracked in-repo at `docs/security/deferred-advisories.md` (GHSA, severity, blocker, unblock trigger)
- Non-dismissal policy: deferred advisories stay `open` on the Security dashboard; do NOT dismiss via the GitHub API
- Spacedrive-scoped Dependabot alerts (under `spacedrive/**`) remain open until the planned runtime integration; re-triaged at integration time
- `.github/dependabot.yml` covers all shipped-code manifests (root cargo, desktop/src-tauri, interface, packages/api-client, spaceui and its sub-manifests, docs); it controls update-PR scoping only, not security-alert visibility

## Deployment
- Docker images published to GHCR (ghcr.io/jrmatherly/spacebot)
- Single binary, no server dependencies
- All data in embedded databases in a local data directory
- Release builds for Linux (x86_64, aarch64), macOS (x86_64, aarch64), Windows (x86_64, aarch64)
