## ADDED Requirements

### Requirement: Outbound HTTP client with explicit safety bounds

Spacebot SHALL provide a `SpacedriveClient` for making outbound HTTP calls to a paired Spacedrive instance. The client MUST set an explicit request `timeout` and `connect_timeout` on its underlying `reqwest::Client`. The client MUST NOT rely on reqwest's default "no timeout" behavior.

#### Scenario: Client construction sets request timeouts

- **WHEN** `SpacedriveClient::new` builds a `reqwest::Client`
- **THEN** the builder MUST pass a non-zero `timeout` and a non-zero `connect_timeout` before calling `.build()`

### Requirement: HTTPS enforcement for non-loopback hosts

The client SHALL reject a plain `http://` base URL whose host is not a loopback address (`localhost`, `127.0.0.1`, `::1`). HTTPS is required for any remote host. The validation MUST happen at client-construction time, not at first request.

#### Scenario: Plain http to a remote host is rejected

- **WHEN** `SpacedriveClient::new` receives a config with `base_url = "http://example.com"`
- **THEN** it MUST return `Err(SpacedriveError::InsecureBaseUrl { host: "example.com" })`

#### Scenario: Plain http to localhost is accepted

- **WHEN** `SpacedriveClient::new` receives a config with `base_url = "http://127.0.0.1:8080"` or `"http://localhost:8080"`
- **THEN** it MUST return `Ok(SpacedriveClient)`

#### Scenario: HTTPS to any host is accepted

- **WHEN** `SpacedriveClient::new` receives a config with `base_url = "https://spacedrive.example.com"`
- **THEN** it MUST return `Ok(SpacedriveClient)`

### Requirement: RPC envelope shape

The client SHALL wrap every `/rpc` request body in either `{"Query": QueryRequest}` or `{"Action": QueryRequest}`. The wire-method prefix passed to `rpc()` determines which: a method string starting with `query:` produces the `Query` envelope; any other prefix produces the `Action` envelope.

#### Scenario: Query-prefixed method produces Query envelope

- **WHEN** the caller invokes `rpc("query:media_listing", payload)`
- **THEN** the serialized request body MUST contain a top-level `Query` key
- **AND** the body MUST NOT contain a top-level `Action` key

#### Scenario: Non-query method produces Action envelope

- **WHEN** the caller invokes `rpc("action:trigger_scan", payload)`
- **THEN** the serialized request body MUST contain a top-level `Action` key
- **AND** the body MUST NOT contain a top-level `Query` key

### Requirement: Bearer token authentication

The client SHALL attach the resolved auth token to every `/rpc` request as a `Bearer` value in the `Authorization` header. The token value MUST come from the `auth_token` parameter passed to `SpacedriveClient::new`; the client MUST NOT read the token directly from any secret store.

#### Scenario: /rpc requests carry the bearer token

- **WHEN** the client sends a `/rpc` request
- **THEN** the request MUST include an `Authorization: Bearer <token>` header matching the value passed to `new`

#### Scenario: Constructor does not touch secret storage

- **WHEN** a caller provides an arbitrary `auth_token` string to `SpacedriveClient::new`
- **THEN** the constructor MUST NOT read from `src/secrets/store.rs` or any external credential source

### Requirement: 401 translation to AuthFailed

A 401 response from Spacedrive SHALL produce `SpacedriveError::AuthFailed`, distinct from other non-success statuses. Callers can use this signal to trigger a token refresh in future phases.

#### Scenario: 401 response produces AuthFailed

- **WHEN** the Spacedrive server returns HTTP 401 to an `/rpc` request
- **THEN** the client MUST return `Err(SpacedriveError::AuthFailed)`
- **AND** it MUST NOT return `SpacedriveError::HttpStatus { status: 401 }`

#### Scenario: Other non-success statuses produce HttpStatus

- **WHEN** the Spacedrive server returns HTTP 500 (or any non-401, non-2xx) to an `/rpc` request
- **THEN** the client MUST return `Err(SpacedriveError::HttpStatus { status: 500 })`

### Requirement: Response-byte cap before deserialization

The client SHALL enforce a response-body size limit on `/rpc` calls before attempting to deserialize the JSON body. The default cap is 10 MB (10 × 1024 × 1024 bytes). A response exceeding the cap MUST produce `SpacedriveError::ResponseTooLarge`.

#### Scenario: Oversized response rejected before parse

- **WHEN** the Spacedrive server returns a 20 MB body to an `/rpc` request and the cap is 10 MB
- **THEN** the client MUST return `Err(SpacedriveError::ResponseTooLarge { actual, cap })`
- **AND** it MUST NOT call `serde_json::from_slice` on the body

### Requirement: Client is ignored when integration is disabled

Nothing SHALL construct a `SpacedriveClient` when `Config.spacedrive.enabled` is `false`. This requirement is forward-looking: Phase 2 adds the client but does not yet call it; Phase 3 adds callers that MUST check `enabled` before attempting to build the client.

#### Scenario: Disabled integration skips client construction

- **WHEN** Spacebot starts with `Config.spacedrive.enabled = false`
- **THEN** no code path MUST call `SpacedriveClient::new`
- **AND** no `/rpc` or `/health` requests MUST be issued to `base_url`
