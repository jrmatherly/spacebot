# Spacebot - Project Overview

**Version:** 0.5.1
**Language:** Rust (edition 2024), ~130K lines of code
**Purpose:** An agentic system where every LLM process has a dedicated role. Replaces the monolithic session model with specialized processes (Channel, Branch, Worker, Compactor, Cortex).

## Tech Stack
- **Runtime:** Tokio async
- **HTTP Framework:** Axum 0.8
- **Database:** SQLite via sqlx 0.8 (48 migrations total: 41 in flat per-agent `migrations/`, 7 in instance-wide `migrations/global/` including `spacedrive_pairing` added 2026-04-17, 2026-02 → 2026-04)
- **Vector DB:** LanceDB 0.27 (embedded vector + FTS for memory)
- **Key-Value Store:** redb 4.0 (embedded)
- **LLM Framework:** Rig v0.35 (agentic loop framework). LiteLLM proxy is a first-class provider as of 2026-04-20 (PR #78; UI Update/Test/Delete modal completed in PR #80 with baseUrl, use_bearer_auth, and extra_headers fields; Azure singular [llm.provider.azure] → plural [llm.providers.azure] migration landed in the same PR with read-fallback for legacy configs): `[llm.providers.litellm]` block with `api_type = "openai_completions"` transport (no new `ApiType` variant), `litellm_api_key` field on `LlmConfig` (System-category, virtual API key — NOT the proxy's admin master key), `ProviderStatus.litellm` bool for UI detection, `litellm/`-prefixed model names skip Spacebot's rate-limit tracking because the LiteLLM Router owns those semantics.
- **CLI:** clap 4.5
- **Serialization:** serde/serde_json 1.0
- **Logging:** tracing 0.1
- **Error Handling:** thiserror + anyhow
- **Discord:** Serenity (rev-pinned from `next` branch at `1cbceb275b10566145b0bdca1c57da9502079a6a` — pulls rustls 0.23 + tokio-tungstenite 0.28 to resolve rustls-webpki audit advisories. Rev-pin replaced branch-tracking 2026-04-19 to stop `cargo update` re-fetches from invalidating incremental cache.)
- **WebSocket:** tokio-tungstenite 0.28
- **Metrics:** Prometheus (feature-gated behind `metrics` feature)
- **macOS Keychain:** security-framework 3

## Frontend
- **Web UI:** Vite + React + TypeScript (`interface/`)
- **JS Package Manager:** bun (NEVER npm/pnpm/yarn)
- **Documentation Site:** Next.js + Fumadocs (`docs/`)
- **SpaceUI:** Design system (6 packages: tokens, primitives, forms, icons, ai, explorer) (`spaceui/`)
- **Internal packages (`@spacebot/*`):** `packages/api-client/` — TypeScript client for the Spacebot REST API + SSE event types. Activated 2026-04-19 (PR #75, commit `3cd80c5`). Consumed by `interface/` via `workspace:*` protocol. Codegen target for `just typegen` / `just check-typegen`. Subpath-only exports (`./client`, `./types`, `./schema`; no root barrel — see `openspec/specs/frontend-api-client/spec.md`). CI guard at `.github/workflows/ci.yml` (`check-typegen` job) validates schema freshness on every PR touching `src/api/**` or `packages/api-client/**`.
- **Spacedrive:** Spacebot-owned fork at `spacedrive/` — independent Cargo workspace with its own toolchain (`stable`), excluded via `[workspace] exclude = ["spacedrive"]` in root `Cargo.toml`. Per the 2026-04-16 self-reliance decision, `spacedrive/` is Spacebot engineering territory; `spacedriveapp/spacedrive` is a historical ancestor, not authority. Track A complete on main as of 2026-04-17: Phase 1 config (PR #54), Phase 2 HTTP client with `{"Query":...}` envelope + HTTPS enforcement + wiremock tests (PR #55), Phase 3 first agent tool `spacedrive_list_files` + prompt-injection envelope + pairing migration + secrets integration (PR #56). Runtime-gated behind `enabled`. Post-merge smoke test (Task 18) was blocked on the vendored fork not compiling; unblocked 2026-04-18 via PR #57 which stubbed 9 declared-but-unwritten sd-core modules + an `apps/web/dist/index.html` placeholder. Smoke test confirmed predicted Bearer/Basic 401 and surfaced a new envelope field-name mismatch (`input` vs `payload`). Both fixes are deferred to a dedicated session that will pair with a real-`sd-server` smoke rerun.
- **Desktop App:** Tauri (`desktop/`)

## Security

- Dependabot alerts in spacebot-owned code that are blocked on upstream crate updates are tracked in-repo at `docs/security/deferred-advisories.md` (GHSA, severity, blocker, unblock trigger)
- Non-dismissal policy: deferred advisories stay `open` on the Security dashboard; do NOT dismiss via the GitHub API
- Spacedrive-scoped Dependabot alerts (under `spacedrive/**`) remain open until the planned runtime integration; re-triaged at integration time
- `.github/dependabot.yml` covers all shipped-code manifests (root cargo, desktop/src-tauri, interface, packages/api-client, spaceui and its sub-manifests, docs); it controls update-PR scoping only, not security-alert visibility
- **Entra ID auth Phase 0 landed 2026-04-20 (PR #81, commit `2a40131`)**: constant-time bearer compare via `subtle = "2"` crate (closes obs #25356); JWT-shape regex added to `LEAK_PATTERNS` at `src/secrets/scrub.rs`; `spacebot_auth_failures_total{branch, reason}` Prometheus counter backed by a typed `AuthRejectReason` enum with 4 reasons (`header_missing`, `header_non_ascii`, `scheme_missing`, `token_mismatch`) and an exhaustiveness-guarded label-snapshot test that compile-fails if a variant is added without a corresponding assertion; CORS-credentials regression test (with positive `access-control-allow-origin` assertion proving the CorsLayer is engaged); `ApiState::new_for_tests` + `api::test_support::build_test_router` always-compiled `#[doc(hidden)]` test scaffolding. Static `auth_token` path preserved unchanged. Phase 1 (JWT middleware) will add the parallel Entra branch. Phase 1 WIP lives on `feat/entra-phase-1-jwt-middleware` (commit `ece2e61` = Task 1.1 app-registrations design doc only, no code yet). See `.scratchpad/plans/entraid-auth/phase-0-hardening.md`, `.scratchpad/plans/entraid-auth/phase-1-jwt-middleware.md`, `docs/design-docs/entra-app-registrations.md`.

## Deployment
- Docker images published to GHCR (ghcr.io/jrmatherly/spacebot)
- Single binary, no server dependencies
- All data in embedded databases in a local data directory
- Release builds for Linux (x86_64, aarch64) and macOS (aarch64 only). Windows was dropped for `daemonize` Unix-only dep + bun-on-Windows preinstall gaps; Intel macOS dropped because `ort-sys` ships no prebuilt for `x86_64-apple-darwin`. Both exclusions are documented inline in `.github/workflows/release.yml`.
