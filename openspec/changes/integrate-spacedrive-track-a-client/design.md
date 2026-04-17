## Context

Track A Phase 1 (`integrate-spacedrive-track-a-config`) landed the `SpacedriveIntegrationConfig` shape and the `[spacedrive]` TOML section. Phase 2 is the first phase with runtime behavior: an outbound HTTP client that lets Spacebot talk to a paired Spacedrive instance.

The wire format is fixed by the in-tree Spacedrive daemon at `spacedrive/crates/sd-client/src/client.rs:50-55`: every `/rpc` request body is a tagged enum with two variants, `{"Query": QueryRequest}` or `{"Action": QueryRequest}`. This is non-negotiable on Spacebot's side; getting it wrong means every request 400s.

The client itself is reusable infrastructure. Phase 3 will build the first agent tool (`spacedrive_list_files`) on top of it, but Phase 2 keeps the scope tight: construct, `health()`, `rpc<I, O>()`. No callers yet.

The plan's original audit log flagged three correctness items:

1. Wire envelope is `{"Query":...}` / `{"Action":...}`, not a `type` discriminator field.
2. Explicit `timeout` + `connect_timeout` required (reqwest defaults to "no timeout").
3. Response-byte cap enforced before `serde_json::from_slice` to prevent OOM from a malicious / misconfigured server.

All three are implemented and testable in this change.

## Goals / Non-Goals

**Goals:**
- Provide a safe-by-default HTTP client (timeouts, HTTPS enforcement, response cap).
- Match Spacedrive's wire envelope so future tool code just works.
- Distinguish 401 from other non-2xx responses so Phase 3 can act on auth failures (token refresh).
- Keep the client's API generic enough to serve multiple tools (`rpc<I: Serialize, O: DeserializeOwned>`).
- Unit test + integration test coverage for construction + envelope shape + auth translation.

**Non-Goals:**
- No callers. Phase 2 adds the client to the binary; nothing calls it yet.
- No token refresh logic. The client surfaces `AuthFailed`; Phase 3 decides what to do with it.
- No retry logic. Phase 2's client does one call, one response. Retries / backoff belong to callers that understand idempotency semantics.
- No streaming responses. Spacedrive's responses are bounded JSON today; streaming is future work.
- No circuit breaker. The client is called on explicit agent-tool invocations, not on a schedule; a circuit breaker would be over-engineering at this scale.

## Decisions

**Decision 1: `timeout = 30s`, `connect_timeout = 5s`.**

30 seconds accommodates a slow Spacedrive response under real load (file listings on large libraries). 5 seconds for connect is aggressive enough to fail fast when the Spacedrive daemon is down but permissive enough to survive transient network blips.

*Alternative considered:* configurable timeouts from `SpacedriveIntegrationConfig`. Rejected because nothing needs to tune them today; a constant is cheaper to maintain. Can be parameterized later without a breaking API change.

**Decision 2: Wire-method prefix routes envelope shape.**

`rpc("query:media_listing", ...)` → `{"Query":...}`; `rpc("action:trigger_scan", ...)` → `{"Action":...}`. The prefix is part of the method string Spacedrive already uses, so the client can do the routing without the caller passing a separate `is_query: bool`.

*Alternative considered:* separate `rpc_query<I, O>()` and `rpc_action<I, O>()` methods. Rejected because it doubles the API surface for a distinction most callers don't care about.

**Decision 3: `auth_token: String` passed to `new`, not loaded inside.**

Per pairing ADR D3, auth tokens live in `src/secrets/store.rs` keyed by `spacedrive_auth_token:<library_id>`. Phase 2's client sidesteps the secret store entirely — the caller (Phase 3) reads the token and hands it to `new()`. This keeps the client unit-testable (no secret store mocking required) and separates wire-handling concerns from credential-resolution concerns.

*Alternative considered:* client loads the token on first request. Rejected because it couples the client to secret-store lifecycles and makes the "no token" error surface ambiguous (at construction vs at first request).

**Decision 4: HTTPS enforcement at construction, not at request time.**

Fail fast: if the base URL is insecure, reject immediately rather than letting a misconfigured deployment sit idle until the first call lands against a plain-http endpoint. The loopback exception matches the common dev-loop pattern (`http://127.0.0.1:8080`).

*Alternative considered:* accept `http://` anywhere and log a warning. Rejected on security grounds — quiet tolerance of plain-http to remote hosts is the kind of thing that ships to production by accident.

**Decision 5: 10 MB response-byte cap.**

Reasoning: agent tools should return structured JSON, not blobs. Spacedrive's largest plausible JSON response in Phase 3 scope (directory listings) is measured in KB, not MB. 10 MB is 3–4 orders of magnitude of headroom; anything close to that limit is almost certainly a bug or attack. The cap runs *before* `serde_json::from_slice` to avoid giving an attacker a free OOM vector.

*Alternative considered:* smaller cap (1 MB) or streaming parser. 1 MB risks cutting off legitimate responses; streaming parser is a bigger API change. 10 MB strikes the balance.

**Decision 6: `RpcEnvelope` uses Rust's default serde enum representation.**

Variant name becomes the JSON key. `RpcEnvelope::Query(QueryRequest)` serializes as `{"Query": {...}}`. No `#[serde(rename_all = ...)]` or `#[serde(tag = ...)]` needed; the default is already PascalCase because the variant names are PascalCase.

*Alternative considered:* `#[serde(rename_all = "PascalCase")]` as the plan originally specified. Rejected as redundant — the variants are already PascalCase.

**Decision 7: Integration tests use wiremock, not mockito or httpmock.**

wiremock is the most widely-used Rust HTTP mock library (11M+ downloads), actively maintained, and has a clean async API. Adding it as a dev-dep only is low-risk.

*Alternative considered:* mockito (older, synchronous API less ergonomic for async code), httpmock (smaller ecosystem). Wiremock's fluent matcher DSL (e.g., `method("POST").and(path("/rpc")).and(header("authorization", "Bearer ..."))`) is the best fit for testing envelope + auth.

## Risks / Trade-offs

- **Risk: Spacedrive's wire envelope changes upstream.** → Mitigation: `spacedrive/SYNC.md` is the provenance discipline that catches upstream drift. If upstream changes the envelope shape, the in-tree fork's diff will flag it at sync time, and the `types.rs` mirror gets updated in a targeted change.

- **Risk: 401 → AuthFailed translation loses information.** → Mitigation: the error includes the inability to distinguish "token invalid" from "token missing" at the wire layer, which is fine because both resolutions go through the same path (reload from secret store). Phase 3's caller can decide whether to retry with a refreshed token.

- **Risk: Response cap cuts off a legitimate large response.** → Mitigation: 10 MB is empirically generous for JSON agent-tool output; if a future tool genuinely needs more, the cap is a constant trivially parameterized. First, show the tool.

- **Trade-off: No retry logic.** Operators get a hard failure on any transient network issue, which is noisier than a silent retry would be. Accepted because retry semantics are call-site-specific (idempotency matters) and belong with the caller, not the client.

- **Trade-off: No connection pooling configuration.** reqwest's default connection pool is used. For a single-paired-Spacedrive integration this is correct; if Phase 3+ spawns many concurrent calls, the default pool size of 100-per-host is comfortable.

## Migration Plan

No runtime migration. The client is added but never constructed. Rollback is a pure revert.

Forward compatibility: when Phase 3 adds callers, no Phase 2 API changes are expected. The client's `new`, `health`, `rpc` surface is stable.

## Open Questions

None. All design questions (envelope shape, auth header, HTTPS enforcement, response cap, retry strategy) are resolved and captured above. Pairing-flow and tool-registration questions are deferred to Phase 3 by design.
