## ADDED Requirements

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
