## 1. Module scaffolding

- [x] 1.1 Create `src/spacedrive/config.rs` with `SpacedriveIntegrationConfig` struct + `Default` impl
- [x] 1.2 Create `src/spacedrive.rs` module root re-exporting `SpacedriveIntegrationConfig`
- [x] 1.3 Add `pub mod spacedrive;` to `src/lib.rs` (between `skills` and `tasks` for alphabetical ordering)
- [x] 1.4 Write unit tests for `SpacedriveIntegrationConfig` default + TOML deserialization (minimal + with reserved fields)
- [x] 1.5 Run `cargo test --lib spacedrive::config::tests` and confirm 3 tests pass

## 2. TOML schema mirror

- [x] 2.1 Add `TomlSpacedriveConfig` struct in `src/config/toml_schema.rs` matching existing `TomlMetricsConfig` pattern
- [x] 2.2 Add `default_spacedrive_base_url()` helper returning `"http://127.0.0.1:8080"`
- [x] 2.3 Add `pub(super) spacedrive: TomlSpacedriveConfig` field to `TomlConfig` between `metrics` and `telemetry`
- [x] 2.4 Implement `Default for TomlSpacedriveConfig` matching `SpacedriveIntegrationConfig::default()`

## 3. Runtime Config wiring

- [x] 3.1 Add `pub spacedrive: crate::spacedrive::SpacedriveIntegrationConfig` field to `Config` in `src/config/types.rs` between `metrics` and `telemetry`
- [x] 3.2 In `src/config/load.rs::from_toml_inner`, convert `toml.spacedrive` to `SpacedriveIntegrationConfig` alongside `metrics` and `telemetry`
- [x] 3.3 Add the field to the main `Ok(Config { ... })` construction in `from_toml_inner`
- [x] 3.4 Add `spacedrive: SpacedriveIntegrationConfig::default()` to the hosted-default `from_env` construction site
- [x] 3.5 Run `cargo check --all-targets` and confirm clean compile

## 4. Round-trip tests

- [x] 4.1 Add `#[cfg(test)] mod tests` block at the end of `src/config/load.rs`
- [x] 4.2 Test `config_round_trips_spacedrive_section` — TOML with `[spacedrive] enabled = true` deserializes correctly
- [x] 4.3 Test `config_omits_spacedrive_section_defaults_disabled` — empty TOML produces `enabled = false`, default `base_url`
- [x] 4.4 Run `cargo test --lib config::load::tests` and confirm both tests pass
- [x] 4.5 Run full `cargo test --lib` and confirm no regressions (expect 824 passing)

## 5. OpenSpec + PR

- [x] 5.1 Create OpenSpec change artifacts (proposal, design, specs, tasks)
- [x] 5.2 Validate OpenSpec proposal passes schema checks (`openspec validate integrate-spacedrive-track-a-config --strict`)
- [x] 5.3 Commit OpenSpec artifacts (one commit; separate from implementation commits for clean history)
- [x] 5.4 Push branch `feat/spacedrive-track-a-config` to origin
- [x] 5.5 Open PR targeting main with title `feat(spacedrive): Track A Phase 1 — config scaffolding` and summary linking the OpenSpec change — PR #54
