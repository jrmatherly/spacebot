## ADDED Requirements

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
