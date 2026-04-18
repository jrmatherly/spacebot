## 1. Envelope helper

- [x] 1.1 Create `src/spacedrive/envelope.rs` with `wrap_spacedrive_response` + helper functions
- [x] 1.2 Define per-tool byte caps: `DEFAULT_CAP`, `CAP_LIST_FILES`, `CAP_READ_FILE`, `CAP_CONTEXT_LOOKUP`
- [x] 1.3 Strip NUL bytes, ANSI escapes, other non-printable controls; preserve `\t\n\r`
- [x] 1.4 Emit truncation marker after closing fence when `raw.len() > byte_cap`
- [x] 1.5 Five unit tests: small-payload wrap, truncation, ANSI strip, NUL strip, JSON round-trip
- [x] 1.6 Re-export from `src/spacedrive.rs`
- [x] 1.7 Activate the deferred anchor in `scripts/check-adr-anchors.sh`
- [x] 1.8 Run `just check-adr-anchors` — green

## 2. Pairing state migration

- [x] 2.1 Create `migrations/global/20260417222250_spacedrive_pairing.sql`
- [x] 2.2 Schema: `library_id UNIQUE`, `spacebot_instance_id`, `spacedrive_base_url`, `paired_at`, `last_seen_at`
- [x] 2.3 No auth-token column — token lives in the secrets store
- [x] 2.4 Add `idx_spacedrive_pairing_library_id` index
- [x] 2.5 `cargo check --all-targets` — clean

## 3. First tool

- [x] 3.1 Create `src/tools/spacedrive_list_files.rs` following `register_file_tools` pattern
- [x] 3.2 Define `SpacedriveListFilesContext` (client + library_id), `SpacedriveListFilesArgs` (path + optional limit), `SpacedriveListFilesTool`
- [x] 3.3 Implement `Tool` trait: `NAME = "spacedrive_list_files"`, `Args`, `Error`, `Output = String`
- [x] 3.4 `call()` invokes `client.rpc("query:media_listing", req)`, wraps response via `wrap_spacedrive_response` with `CAP_LIST_FILES`
- [x] 3.5 Export `register_spacedrive_tools(server, client, library_id) -> ToolServer` factory
- [x] 3.6 Register `pub mod spacedrive_list_files` in `src/tools.rs` (alphabetical)
- [x] 3.7 Add `#[derive(Debug)]` to `SpacedriveClient` (needed for Context type)
- [x] 3.8 Write paired prompt at `prompts/en/tools/spacedrive_list_files_description.md.j2` (per tool-authoring rule)
- [x] 3.9 Switch tool description to `prompts::text::get("tools/spacedrive_list_files")`
- [x] 3.10 Two unit tests: args round-trip, limit is optional

## 4. Secret store integration + runtime config plumbing

- [x] 4.1 Add `pub spacedrive: SpacedriveIntegrationConfig` field to `RuntimeConfig` (plain, not ArcSwap — instance-wide, immutable after startup)
- [x] 4.2 Add `spacedrive` parameter to `RuntimeConfig::new`
- [x] 4.3 Update 5 call sites: `src/main.rs`, 2× `src/api/agents.rs`, `tests/context_dump.rs`, `tests/bulletin.rs`
- [x] 4.4 Create a helper to read fresh `spacedrive` from disk alongside `disk_defaults` in the create-agent handler
- [x] 4.5 Add `build_client_from_secrets(cfg, secrets) -> Result<SpacedriveClient>` to `src/spacedrive.rs`
- [x] 4.6 Key format `spacedrive_auth_token:<library_id>` per pairing ADR D3
- [x] 4.7 Returns `MissingAuthToken` if secret absent, `Disabled` if `library_id.is_none()`

## 5. Tool registration wiring

- [x] 5.1 Wire registration at `create_worker_tool_server` (line ~898)
- [x] 5.2 Wire registration at `create_cortex_chat_tool_server` (line ~1073)
- [x] 5.3 Gate on `enabled && library_id.is_some() && secrets.load().is_some()`
- [x] 5.4 Failed client build → `tracing::warn!` + skip, no panic
- [x] 5.5 `cargo test --lib` — 837 pass (+8 from Phase 2 baseline)

## 6. End-to-end test

- [x] 6.1 Append `#[tokio::test] list_files_wraps_response_in_envelope` to `src/tools/spacedrive_list_files.rs`
- [x] 6.2 Stand up wiremock server returning `{"data": {"files": [{"name": "report.pdf"}]}}`
- [x] 6.3 Build real `SpacedriveClient`, construct `SpacedriveListFilesTool`, call with `path = "/"`
- [x] 6.4 Assert output starts with provenance tag, contains both fence markers, contains `report.pdf`

## 7. Docs

- [x] 7.1 Add combined Track A Phase 1-3 entry to `CHANGELOG.md` under `## Unreleased` → `### Added`
- [x] 7.2 Reference ADRs (pairing + envelope) as landed in Phase 0
- [x] 7.3 Note runtime-gated behavior: no operator-visible change without opt-in

## 8. OpenSpec + PR

- [x] 8.1 Create change artifacts (proposal, design, specs, tasks)
- [ ] 8.2 Validate with `openspec validate integrate-spacedrive-track-a-tool-list-files --strict`
- [ ] 8.3 Commit OpenSpec artifacts
- [ ] 8.4 Push branch `feat/spacedrive-track-a-tool-list-files` to origin
- [ ] 8.5 Open PR targeting main; reference Phase 3 in title
