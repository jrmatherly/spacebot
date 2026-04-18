## Why

Track A Phases 1 and 2 landed the config shape and the HTTP client, but nothing yet reads them. Phase 3 closes the loop: the first Spacedrive-backed agent tool, the prompt-injection defense envelope every future Spacedrive tool will share, the pairing state migration, and the secret-store integration that turns "client can talk to Spacedrive" into "agents can use it when an operator opts in."

This is the first phase with an agent-visible surface. Operators who enable `[spacedrive] enabled = true`, complete a pairing flow (future work), and populate the `spacedrive_auth_token:<library_id>` secret will see `spacedrive_list_files` in their agents' tool roster.

## What Changes

- `src/spacedrive/envelope.rs`: `wrap_spacedrive_response(library_id, wire_method, raw, byte_cap)` produces a prompt-injection-safe wrapper with provenance tag, untrusted-content fences, byte-cap truncation, and control-char stripping. Per-tool caps exposed (`CAP_LIST_FILES = 64K`, `CAP_READ_FILE = 1MB`, `CAP_CONTEXT_LOOKUP = 16K`, `DEFAULT_CAP = 10MB`).
- `src/tools/spacedrive_list_files.rs`: first Spacedrive-backed tool. Calls `query:media_listing`, wraps the JSON response via the envelope. Paired prompt at `prompts/en/tools/spacedrive_list_files_description.md.j2` per the tool-authoring rule.
- `src/spacedrive.rs`: `build_client_from_secrets(&cfg, &secrets) -> Result<Client>` reads the auth token from the secrets store under `spacedrive_auth_token:<library_id>`.
- `src/config/runtime.rs`: new `pub spacedrive: SpacedriveIntegrationConfig` field on `RuntimeConfig` (plain, not ArcSwap — instance-wide, follows the `instance_dir` precedent). `RuntimeConfig::new` gains a parameter; updated at five call sites (main, 2× api/agents, 2× tests).
- `src/tools.rs`: registration blocks at both `create_worker_tool_server` and `create_cortex_chat_tool_server`. Both gated on `enabled && library_id.is_some() && secrets store present`. Failures log WARN and skip.
- `migrations/global/20260417222250_spacedrive_pairing.sql`: instance-wide pairing state table (`library_id UNIQUE`, `spacebot_instance_id`, `spacedrive_base_url`, timestamps). Auth token explicitly not stored.
- `scripts/check-adr-anchors.sh`: activates the deferred envelope-helper anchor slot reserved in PR #53.
- `CHANGELOG.md`: single entry summarizing all three Track A phases.

Nothing runs by default. `spacedrive.enabled` is `false` out of the box; pairing flow doesn't exist yet; no operator is affected without explicit configuration.

## Capabilities

### New Capabilities

None. This extends the existing `spacedrive-integration` capability (introduced by Phase 1, extended with HTTP client in Phase 2).

### Modified Capabilities

- `spacedrive-integration`: adds the agent-facing tool surface, the prompt-injection envelope contract, pairing state persistence, and the secret-store integration binding. These are all new requirements added on top of the Phase 1 and Phase 2 shape.

## Impact

- **Code**: new `src/spacedrive/envelope.rs`, `src/tools/spacedrive_list_files.rs`, `migrations/global/20260417222250_spacedrive_pairing.sql`, `prompts/en/tools/spacedrive_list_files_description.md.j2`. Modified `src/spacedrive.rs`, `src/config/runtime.rs`, `src/tools.rs`, `src/main.rs`, `src/api/agents.rs`, `tests/context_dump.rs`, `tests/bulletin.rs`, `scripts/check-adr-anchors.sh`, `CHANGELOG.md`.
- **APIs**: new public symbols `wrap_spacedrive_response`, `build_client_from_secrets`, `register_spacedrive_tools`, tool name `spacedrive_list_files`. New field `RuntimeConfig::spacedrive`. New param on `RuntimeConfig::new`.
- **Dependencies**: none new. `wiremock` already in dev-deps from Phase 2.
- **Behavior**: zero default-behavior change. Tool only registers when operator opts in AND pairing state exists AND secret is set.
- **Tests**: +8 lib tests (5 envelope + 3 tool) +1 e2e integration (list_files wraps response). Lib suite 830 → 838.
- **Migrations**: +1 in `migrations/global/` (pairing table).
