## Why

Track A Phase 1 landed the `SpacedriveIntegrationConfig` shape but no runtime behavior. Phase 2 adds the missing piece: an outbound HTTP client that talks to a paired Spacedrive instance over `POST /rpc` using Spacedrive's `{"Query":...}` / `{"Action":...}` envelope. This is still pre-agent-tool work — the client is reusable infrastructure that Phase 3's first agent tool (`spacedrive_list_files`) will call. Shipping it as its own change keeps the client's review focused on wire-format and safety decisions (timeouts, HTTPS enforcement, response caps) without mixing in agent-tool concerns.

## What Changes

- `src/spacedrive/error.rs`: `SpacedriveError` enum via `thiserror` covering the full wire-error surface (`Disabled`, `MissingAuthToken`, `AuthFailed`, `HttpStatus`, `ResponseTooLarge`, `InsecureBaseUrl`, `Wire`) plus `#[from]` wrappers for `reqwest::Error` and `serde_json::Error`. Companion `Result<T>` alias.
- `src/spacedrive/types.rs`: Wire-shape mirrors — `RpcEnvelope` (the `{"Query":...}` / `{"Action":...}` outer wrapper), `QueryRequest` (payload), `RpcResponse<T>` (generic response), `HealthResponse`. Spacebot-owned, not imported from `spacedrive::*`, per self-reliance guarantees.
- `src/spacedrive/client.rs`: `SpacedriveClient` with:
  - Explicit `timeout = 30s`, `connect_timeout = 5s`.
  - `https://` enforcement for non-loopback hosts.
  - 10 MB response-byte cap enforced before JSON deserialization.
  - Methods: `new(cfg, auth_token) -> Result<Self>`, `health()`, `rpc<I, O>(wire_method, payload)`.
  - `wire_method` prefix routes the envelope: `query:...` → `RpcEnvelope::Query`, anything else → `RpcEnvelope::Action`.
- `tests/spacedrive_client.rs`: wiremock-backed integration tests for health, envelope shape, and 401 → `AuthFailed` translation.
- `Cargo.toml`: adds `wiremock = "0.6"` to `[dev-dependencies]`.

Nothing reads the client yet. Phase 3 will be the first caller.

No breaking changes. No runtime behavior for operators who haven't enabled the integration.

## Capabilities

### New Capabilities

None. This extends the existing `spacedrive-integration` capability (introduced by `integrate-spacedrive-track-a-config`).

### Modified Capabilities

- `spacedrive-integration`: Adds outbound HTTP client behavior to the capability. The config-only shape from Phase 1 gains its first runtime-callable surface. Requirements covering client construction, wire envelope, auth handling, and safety bounds (timeouts, response cap, HTTPS enforcement) land here.

## Impact

- **Code**: new `src/spacedrive/error.rs`, `src/spacedrive/types.rs`, `src/spacedrive/client.rs`, `tests/spacedrive_client.rs`. Modified `src/spacedrive.rs` (re-exports), `Cargo.toml` (`wiremock` dev-dep), `Cargo.lock`.
- **APIs**: `SpacedriveClient::new`, `health`, `rpc` become callable by any future module. No callers yet.
- **Dependencies**: `wiremock 0.6` (dev-only). Already-present crates used: `reqwest`, `serde`, `serde_json`, `thiserror`, `tokio`, `tracing`, `url`, `uuid`.
- **Behavior**: still zero runtime effect for operators. Construction will happen only from Phase 3 code paths when `spacedrive.enabled = true`.
- **Tests**: +6 lib tests (3 types + 3 client construction) and +3 integration tests. Total lib suite goes 824 → 830.
- **Migrations**: none. Pairing migration lands in Phase 3.
