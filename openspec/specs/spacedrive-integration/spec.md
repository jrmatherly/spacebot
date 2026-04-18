# spacedrive-integration Specification

## Purpose
TBD - created by archiving change integrate-spacedrive-track-a-config. Update Purpose after archive.
## Requirements
### Requirement: Spacedrive integration config section

Spacebot SHALL expose a top-level `[spacedrive]` section in its TOML configuration, materialized as a `SpacedriveIntegrationConfig` value on the runtime `Config` struct. The section SHALL be fully omittable; absent sections MUST resolve to a disabled default.

#### Scenario: Absent section defaults to disabled

- **WHEN** a Spacebot config TOML does not contain a `[spacedrive]` section
- **THEN** the loaded `Config.spacedrive.enabled` MUST be `false`
- **AND** `Config.spacedrive.base_url` MUST be `"http://127.0.0.1:8080"`
- **AND** `Config.spacedrive.library_id` MUST be `None`
- **AND** `Config.spacedrive.spacebot_instance_id` MUST be `None`

#### Scenario: Minimal enabled section round-trips

- **WHEN** a Spacebot config TOML contains `[spacedrive]\nenabled = true\nbase_url = "http://127.0.0.1:8080"`
- **THEN** the loaded `Config.spacedrive.enabled` MUST be `true`
- **AND** the loaded `Config.spacedrive.base_url` MUST equal the TOML value

### Requirement: Reserved pairing-state fields

The `[spacedrive]` section SHALL reserve `library_id` and `spacebot_instance_id` as optional UUID fields populated by the pairing flow (future Phase 3). The config shape MUST accept these fields from TOML when present, but they MUST NOT be hand-edited as part of normal operator workflow.

#### Scenario: Pairing fields accepted when present

- **WHEN** a config TOML contains `library_id = "a1b2c3d4-1234-5678-9abc-def012345678"` inside `[spacedrive]`
- **THEN** the loaded `Config.spacedrive.library_id` MUST be `Some(Uuid::parse_str("a1b2c3d4-1234-5678-9abc-def012345678").unwrap())`

#### Scenario: Pairing fields default to None when absent

- **WHEN** a config TOML includes `[spacedrive]` but omits `library_id` and `spacebot_instance_id`
- **THEN** both fields on the loaded `Config.spacedrive` MUST be `None`

### Requirement: Auth token stays out of TOML

The Spacedrive integration's auth token SHALL NOT appear as a TOML-visible field. The config struct MUST NOT expose the token as a serializable field. The token is instead resolved from Spacebot's secret store at client-construction time using the key format `spacedrive_auth_token:<library_id>` per pairing ADR decision D3.

#### Scenario: Auth token field absent from config schema

- **WHEN** inspecting `SpacedriveIntegrationConfig`'s fields
- **THEN** there MUST NOT be a field named `auth_token`, `token`, `secret`, or any equivalent

#### Scenario: Auth token lookup deferred

- **WHEN** Phase 2 or later client code needs the token
- **THEN** the token MUST be resolved from the secret store keyed by the library ID, not from `Config.spacedrive`

### Requirement: Runtime disabled by default

The integration SHALL contribute no runtime behavior when `Config.spacedrive.enabled` is `false`. The module SHALL compile and be reachable at all times, but callers MUST check the `enabled` flag before starting client or tool work.

#### Scenario: Disabled integration has no runtime effect

- **WHEN** Spacebot starts with `Config.spacedrive.enabled = false`
- **THEN** no HTTP client is constructed
- **AND** no Spacedrive-backed agent tools are registered
- **AND** no connection attempts to the `base_url` are made

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

### Requirement: Prompt-injection envelope for Spacedrive-returned bytes

Every agent tool that returns Spacedrive-originated content to the LLM SHALL wrap the content via `wrap_spacedrive_response`. The envelope MUST include a provenance tag, fences that mark the content as untrusted, byte-cap truncation, and control-character stripping.

#### Scenario: Envelope carries provenance tag

- **WHEN** `wrap_spacedrive_response("lib-x", "query:media_listing", payload, cap)` is called
- **THEN** the returned string MUST start with `[SPACEDRIVE:lib-x:query:media_listing]`

#### Scenario: Envelope wraps payload in untrusted-content fences

- **WHEN** the envelope is constructed
- **THEN** the output MUST contain `<<<UNTRUSTED_SPACEDRIVE_CONTENT>>>` before the payload
- **AND** the output MUST contain `<<<END_UNTRUSTED_SPACEDRIVE_CONTENT>>>` after the payload

#### Scenario: Oversized payload is truncated

- **WHEN** `raw` exceeds `byte_cap`
- **THEN** the envelope MUST include a trailing marker `[...truncated, original size N bytes, cap M]`
- **AND** only the first `byte_cap` bytes (minus any whole-character trim) MUST appear inside the fences

#### Scenario: ANSI escapes stripped

- **WHEN** `raw` contains ANSI color escapes (e.g., `\x1b[31m`)
- **THEN** the envelope output MUST NOT contain `\x1b` characters

#### Scenario: NUL bytes stripped

- **WHEN** `raw` contains NUL bytes
- **THEN** the envelope output MUST NOT contain NUL characters

### Requirement: spacedrive_list_files agent tool

Spacebot SHALL expose a `spacedrive_list_files` agent tool that calls the paired Spacedrive's `query:media_listing` RPC and returns a listing wrapped in the envelope. The tool's description SHALL be loaded from `prompts/en/tools/spacedrive_list_files_description.md.j2` per the tool-authoring convention.

#### Scenario: Tool name is fixed

- **WHEN** the tool's `Tool::NAME` constant is read
- **THEN** it MUST equal `"spacedrive_list_files"`

#### Scenario: Tool description loads from paired prompt

- **WHEN** the tool's `definition()` is invoked
- **THEN** the `description` MUST equal `prompts::text::get("tools/spacedrive_list_files")`
- **AND** the tool MUST NOT hard-code the description string inline

#### Scenario: Response wrapped through envelope

- **WHEN** the tool receives a successful Spacedrive response
- **THEN** the output MUST pass through `wrap_spacedrive_response` with the `CAP_LIST_FILES` cap (64 KB)
- **AND** the output MUST begin with `[SPACEDRIVE:<library_id>:query:media_listing]`

### Requirement: Paired pairing-state persistence

Spacebot SHALL persist Spacedrive pairing state in the instance-wide (`migrations/global/`) database. The schema MUST enforce one pairing per library via `UNIQUE(library_id)`. The auth token MUST NOT be stored in this table — it lives in the secrets store under `spacedrive_auth_token:<library_id>` per pairing ADR D3.

#### Scenario: Pairing table constraints

- **WHEN** two rows with the same `library_id` are inserted into `spacedrive_pairing`
- **THEN** the second insert MUST fail with a UNIQUE constraint violation

#### Scenario: Pairing table does not carry secrets

- **WHEN** inspecting the `spacedrive_pairing` schema
- **THEN** there MUST NOT be a column named `auth_token`, `token`, `secret`, or `api_key`

### Requirement: Secret-store-backed client construction

Spacebot SHALL expose `build_client_from_secrets(cfg, secrets)` that constructs a `SpacedriveClient` by reading the auth token from the secrets store under the key format `spacedrive_auth_token:<library_id>`. Missing tokens MUST surface as `SpacedriveError::MissingAuthToken`. Unpaired configurations (missing `library_id`) MUST surface as `SpacedriveError::Disabled`.

#### Scenario: Missing token surfaces MissingAuthToken

- **WHEN** `build_client_from_secrets` is called with a config whose `library_id` is `Some(X)` but no `spacedrive_auth_token:X` exists in the secrets store
- **THEN** the call MUST return `Err(SpacedriveError::MissingAuthToken { library_id })`

#### Scenario: Missing library_id surfaces Disabled

- **WHEN** `build_client_from_secrets` is called with a config whose `library_id` is `None`
- **THEN** the call MUST return `Err(SpacedriveError::Disabled)`

### Requirement: Runtime tool registration gate

The `spacedrive_list_files` tool SHALL be registered on a ToolServer only when all three conditions hold: `spacedrive.enabled` is `true`, `spacedrive.library_id` is `Some`, and a `SecretsStore` is loaded on the `RuntimeConfig`. A failure to build the client MUST log a WARN and skip registration without panicking or aborting server construction.

#### Scenario: Disabled integration skips registration

- **WHEN** a ToolServer is constructed with `runtime_config.spacedrive.enabled = false`
- **THEN** the server MUST NOT include the `spacedrive_list_files` tool

#### Scenario: Missing library_id skips registration

- **WHEN** a ToolServer is constructed with `enabled = true` but `library_id = None`
- **THEN** the server MUST NOT include the `spacedrive_list_files` tool

#### Scenario: Missing secrets store skips registration

- **WHEN** a ToolServer is constructed with `enabled = true`, `library_id = Some`, but `runtime_config.secrets.load()` returns `None`
- **THEN** the server MUST NOT include the `spacedrive_list_files` tool

#### Scenario: Failed client build surfaces as warning

- **WHEN** `build_client_from_secrets` returns `Err` at registration time
- **THEN** registration MUST log a `tracing::warn!` with the error
- **AND** MUST NOT add the tool to the server
- **AND** MUST NOT propagate the error out of the tool-server builder

