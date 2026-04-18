## 1. Error module

- [x] 1.1 Create `src/spacedrive/error.rs` with `SpacedriveError` enum via `thiserror`
- [x] 1.2 Add variants: `Disabled`, `MissingAuthToken`, `AuthFailed`, `HttpStatus`, `ResponseTooLarge`, `InsecureBaseUrl`, `Wire`, `Http(#[from] reqwest::Error)`, `Json(#[from] serde_json::Error)`
- [x] 1.3 Add `Result<T>` type alias
- [x] 1.4 Re-export from `src/spacedrive.rs`

## 2. Wire types

- [x] 2.1 Create `src/spacedrive/types.rs` with `RpcEnvelope::{Query, Action}` tagged enum
- [x] 2.2 Add `QueryRequest` with `method: String`, `library_id: Option<Uuid>`, `payload: serde_json::Value`
- [x] 2.3 Add generic `RpcResponse<T>` with optional `data` and `error` fields
- [x] 2.4 Add `HealthResponse` typed wrapper
- [x] 2.5 Unit tests: Query envelope serializes with `Query` key, Action with `Action` key, RpcResponse deserializes empty-object JSON cleanly

## 3. HTTP client

- [x] 3.1 Create `src/spacedrive/client.rs` with `SpacedriveClient` struct
- [x] 3.2 Implement `new(cfg, auth_token) -> Result<Self>` with base_url parsing + HTTPS-or-loopback enforcement
- [x] 3.3 Build `reqwest::Client` with `timeout = 30s`, `connect_timeout = 5s`
- [x] 3.4 Implement `health()` → `GET /health` returning `HealthResponse`
- [x] 3.5 Implement `rpc<I: Serialize, O: DeserializeOwned>(wire_method, payload) -> Result<O>` with envelope prefix routing
- [x] 3.6 Attach `Authorization: Bearer <token>` on `/rpc` requests
- [x] 3.7 Translate 401 → `AuthFailed`, other non-2xx → `HttpStatus`
- [x] 3.8 Enforce 10 MB response-byte cap before JSON parse
- [x] 3.9 Re-export `SpacedriveClient` from `src/spacedrive.rs`

## 4. Unit tests

- [x] 4.1 Test: plain-http to non-loopback host rejected with `InsecureBaseUrl`
- [x] 4.2 Test: plain-http to `127.0.0.1` accepted
- [x] 4.3 Test: HTTPS to any host accepted

## 5. Integration tests

- [x] 5.1 Add `wiremock = "0.6"` to `[dev-dependencies]` in `Cargo.toml`
- [x] 5.2 Create `tests/spacedrive_client.rs` with three scenarios
- [x] 5.3 Test: `health()` against a mock `GET /health` returning "OK"
- [x] 5.4 Test: `rpc("query:media_listing", ...)` sends the bearer token and parses the JSON response
- [x] 5.5 Test: 401 response translates to `SpacedriveError::AuthFailed`
- [x] 5.6 Run `cargo test --test spacedrive_client` and confirm 3 tests pass

## 6. OpenSpec + PR

- [x] 6.1 Create OpenSpec change artifacts (proposal, design, specs, tasks)
- [x] 6.2 Validate with `openspec validate integrate-spacedrive-track-a-client --strict`
- [x] 6.3 Commit OpenSpec artifacts
- [x] 6.4 Push branch `feat/spacedrive-track-a-client` to origin
- [x] 6.5 Open PR targeting main
