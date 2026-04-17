## Why

Spacebot has the Spacedrive platform vendored in-tree (see `openspec/specs/spacedrive-in-tree/`), but no runtime integration surface exists. Before any HTTP client, tool, or pairing flow can land, Spacebot needs a typed configuration shape the rest of the integration can read from. Shipping that shape first, with zero runtime behavior, is the smallest piece that unblocks the next phases without committing to any specific wire format or auth flow yet.

## What Changes

- New `src/spacedrive` module containing the `SpacedriveIntegrationConfig` struct.
- New `[spacedrive]` top-level TOML section on Spacebot's config (`enabled`, `base_url`, reserved `library_id` / `spacebot_instance_id`).
- `[spacedrive]` wired into `src/config/types.rs` (`Config` struct) and `src/config/load.rs` (both construction paths: `from_toml_inner` and the hosted-default `from_env`).
- `TomlSpacedriveConfig` mirror added to `src/config/toml_schema.rs` following the existing `TomlMetricsConfig` / `TomlTelemetryConfig` pattern.
- Round-trip tests verifying the section deserializes correctly when present and defaults to disabled when absent.

Not in scope (deferred to later phases):

- HTTP client (`src/spacedrive/client.rs`) — Phase 2.
- Error types, RPC envelope types — Phase 2.
- First tool (`spacedrive_list_files`), pairing migration, secret-store integration — Phase 3.

No breaking changes. `enabled` defaults to `false`; unconfigured instances are unaffected.

## Capabilities

### New Capabilities

- `spacedrive-integration`: The runtime surface Spacebot uses to talk to a paired Spacedrive instance. This change introduces the capability at its config layer only; subsequent changes extend it with the HTTP client, RPC envelope types, prompt-injection defense envelope, and the first agent tool.

### Modified Capabilities

None.

## Impact

- **Code**: New `src/spacedrive.rs` and `src/spacedrive/config.rs`. Modified `src/lib.rs`, `src/config/types.rs`, `src/config/toml_schema.rs`, `src/config/load.rs`.
- **APIs**: None yet. The config field is readable programmatically via `Config::spacedrive`, but nothing reads it in this change.
- **Dependencies**: No new crates. `uuid` and `toml` were already in `Cargo.toml`.
- **Behavior**: None. The integration is runtime-gated behind `SpacedriveIntegrationConfig::enabled`, which defaults to `false`.
- **Docs**: References the pairing ADR at `docs/design-docs/spacedrive-integration-pairing.md` (D2, D3) and the envelope ADR at `docs/design-docs/spacedrive-tool-response-envelope.md`. Neither is modified.
- **Migrations**: None. The pairing migration lands in Phase 3.
