# Phase 8 PR A — Tauri Desktop Auth (Shipped 2026-04-24)

**Status:** Merged to main as squash commit `7cbfb8d` (PR #117).
**Branch:** `feat/entra-phase-8-pr-a-tauri-loopback` (deleted).

## What shipped

### Desktop side (`desktop/src-tauri/`)
- **`src/auth.rs`** — CSPRNG state, PKCE S256 (32-byte verifier, SHA-256 challenge), v2.0 authorize URL builder, hand-rolled hyper-less loopback HTTP listener on `127.0.0.1` in the pre-registered `50000..=50009` port range, one-shot HTTP callback parser with state-equality CSRF check and 5-minute timeout, bounded bad-request budget (16), v2.0 token endpoint client. All helpers return `anyhow::Result` with `.context()`; collapse to String at Tauri boundary.
- **`src/main.rs`** — `sign_in_with_entra` Tauri command composing the primitives, opening system browser via `tauri-plugin-opener`, forwarding tokens to daemon. Splits 503/403/other status codes into distinct user messages. Separate `sign_in_with_entra_inner` function returns `anyhow::Result` so the outer command boundary collapses once with `format!("{e:#}")`.
- **`capabilities/default.json`** — added `opener:allow-open-url`.
- **`Cargo.toml`** — added `tauri-plugin-opener`, `rand 0.10`, `reqwest`, `url`, `tokio`, `base64`, `sha2`, `anyhow`. (The original `hyper`/`hyper-util`/`http-body-util` deps were removed in remediation — implementation uses raw `tokio::net::TcpStream`.)
- **6 unit tests** at `auth_tests.rs`: state URL-safety, state length ≥32, state uniqueness, PKCE S256 relation, bind_loopback range bound, authorize URL param completeness.

### Daemon side (`src/`)
- **`src/api/desktop.rs`** (new, ~170 lines) — `POST /api/desktop/tokens` handler with three-layer defense:
  1. Peer IP must satisfy `is_loopback()` (rejects non-127.0.0.1/::1)
  2. `Host` header must match `127.0.0.1` / `[::1]` / `localhost` via `is_loopback_host(raw: &str)` helper (defends DNS-rebinding; handles bracketed IPv6 correctly — this was a live bug caught by the new test suite)
  3. Tokens land in `SecretCategory::System`; locked store surfaces as 503 via `classify_secret_write` helper matching on the newly-promoted `SecretsError::StoreLocked` variant
  - **Atomicity:** on refresh_token write failure after access_token succeeds, `rollback_access_token` deletes the stranded access_token. Rollback failure logged but doesn't override the original status.
- **`src/secrets/store.rs`** — promoted `SecretsError::StoreLocked` variant (was `SecretsError::Other(anyhow!("...locked..."))` at 4 sites: `set`, `get`, `export_all`, `import_all`). Variant already existed at `src/error.rs:235` but was never constructed. Typed match replaces string-contains check at the handler.
- **`src/auth/bypass.rs`** — `/api/desktop/tokens` added to `AUTH_BYPASS_PATHS` at index 1 (sorted order between `/api/auth/config` and `/api/health`). Bypass is path-only, not verb-aware, so future GET inherits automatically.
- **`src/api/server.rs`** — `axum::serve(listener, app)` flipped to `axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())` to light up the `ConnectInfo<SocketAddr>` extractor.
- **`packages/api-client/src/schema.d.ts`** — regenerated from utoipa annotations.

### Tests
- **`tests/api_desktop_tokens.rs`** (new, 7 integration tests via `axum::MockConnectInfo`):
  - `rejects_non_loopback_peer` → 403
  - `rejects_attacker_host_header` → 403
  - `accepts_all_three_loopback_host_names` → IPv6 `[::1]`, IPv4, localhost all pass
  - `returns_503_when_store_locked` — uses `enable_encryption` + `lock()`
  - `returns_500_when_store_absent`
  - `persists_access_and_refresh_tokens` — round-trip assertion via `SecretsStore::get`
  - `accepts_missing_refresh_token`
- **`tests/api_auth_middleware.rs`** — added `desktop_tokens_bypasses_token_check` (static-token middleware branch)
- **`tests/entra_jwt_middleware.rs::router_level`** — added `desktop_tokens_bypasses_entra_jwt_check` (Entra JWT middleware branch)

Both bypass tests discharge the explicit `bypass.rs:35-38` docstring obligation that a new allowlist entry be regression-tested against BOTH middleware branches.

## Multi-reviewer remediation (commit `a55497f`)

Five review agents (code-reviewer, pr-test-analyzer, silent-failure-hunter, type-design-analyzer, comment-analyzer) produced 24 findings; all applied. One bonus bug caught by the new test suite during remediation: the original Host-header `split(':')` approach never matched bracketed IPv6 literals like `[::1]` because every IPv6 addr contains colons. Fixed via the dedicated `is_loopback_host()` helper.

**Reviewer finding partially disputed:** I-2 (rand import) suggested `use rand::RngCore`. Tested: rand 0.10 moved `RngCore` to the `rand_core` crate; `rand::RngCore` doesn't resolve. Kept `use rand::Rng` with comment explaining that `fill_bytes` reaches `ThreadRng` via the `Rng` extension trait supertrait chain in 0.10.

## Post-merge state

- `main` HEAD: `7cbfb8d` (PR #117 squash)
- 924+ lib tests pass, 31+ integration test binaries compile, `just gate-pr` green
- `interface/dist/` has no Phase 8 changes (PR B territory)

## What's next: Phase 8 PR B

Plan file: `.scratchpad/plans/entraid-auth/phase-8-tauri-auth.md` (gitignored, 1523 lines after post-PR-A amendments).

The plan was amended 2026-04-24 with 10 findings from a pre-PR-B audit:
- **Task 8.B.0 (NEW):** Daemon-side `GET /api/desktop/tokens` read + `clear_auth_tokens` endpoint + Tauri `get_cached_access_token` command + shared `enforce_loopback_preconditions` helper extracted from the existing POST handler. Two options documented: DRT-A (ship for acceptance criterion 4) or DRT-B (defer to Phase 9, cold start always shows sign-in).
- **Task 8.B.0.5 (NEW):** Invert `App.tsx` provider layering so `<AuthGate>` runs inside `<ServerProvider>`. Adds `waiting_for_server` state to `GateState` at `AuthGate.tsx:32-37`. Fixes Tauri cold-start `loadAuthConfig` race (SPA serves from `tauri://localhost` in desktop, so relative `/api/auth/config` never reached the daemon).
- **Task 8.B.1 (REWRITTEN):** Add `platform.invokeCommand<T>()` helper at `interface/src/platform.ts`. `tauriBridge.ts` routes through it; `isTauri()` deprecated in favor of `IS_DESKTOP`. Per the module-level rule at `platform.ts:6` banning direct `@tauri-apps/*` imports.
- **Task 8.B.2 (REWRITTEN):** Added Step 0 grep-audit of `MsalProvider`'s required method surface against installed `@azure/msal-react`. Tauri branch insertion pinned precisely: after `msalConfig.ts:108` (mock branch), before line 111 (first PCA statement). Shim uses typed `AccountInfo | null` signatures matching `mockMsal.ts:85` precedent. Seeds synthetic `AccountInfo` from cached token on cold start so `getAllAccounts()` isn't empty.
- **Task 8.B.2.5 (NEW):** vitest for the shim — 5 tests pinning method surface + sign-in call-through.
- **Task 8.B.3 (RESCOPED):** `ConnectionScreen.tsx` is **NOT modified**. `AuthGate.SignInPrompt` is the single sign-in surface (prevents competing `needs_auth` UI collision). `SignInPrompt` gets IS_DESKTOP-aware copy + anyhow-chain error rendering.
- **Task 8.B.4 (UNCHANGED):** App-registration docs for 10 redirect URIs.
- **Task 8.B.5 (ENHANCED):** Added writing-guide em-dash sweep on commit messages before PR open.

### PR B's open decision

**Task 8.B.0 Option DRT-A vs DRT-B** must be chosen first — determines whether acceptance criterion 4 ("SPA AuthenticatedTemplate renders on cold start with cached token") is in scope. Recommend DRT-A.

## Deferred to post-Phase-8

- `src/api/secrets.rs:672` and `:737` still use `error.to_string().contains("locked")` for HTTP 423. Now that `SecretsError::StoreLocked` is a named variant, these can be simplified in a follow-up cleanup PR.
- Expiry tracking for `entra_access_token` (currently only stores the token string; `expires_in` is accepted from the wire but not persisted).
- Refresh-token rotation before expiry; offline-grace.
- `ResourceType(&'static str)` newtype (from Phase 7 polish backlog).

## Known handoff artifacts

- **This memory** (`phase_8_pr_a_shipped`) — primary PR A reference.
- **`.scratchpad/plans/entraid-auth/phase-8-tauri-auth.md`** — 1523-line amended plan with all 10 audit findings + 3 auditor-surfaced drift corrections applied.
- **Code-review graph** updated incrementally for the 15 files in PR #117 (186 nodes, 2300 edges for this PR's scope).
- **No `.scratchpad/session-primer/phase-8-pr-b-resume.md`** yet — that's the next session's first artifact.
