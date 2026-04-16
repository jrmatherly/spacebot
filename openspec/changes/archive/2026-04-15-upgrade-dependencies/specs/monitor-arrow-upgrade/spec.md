## ADDED Requirements

### Requirement: Arrow upgrade blocked upstream
The system SHALL remain on `arrow-array` 57 and `arrow-schema` 57 until lancedb publishes a version supporting arrow 58 to crates.io. No manual version override or feature patching SHALL be attempted.

#### Scenario: No premature arrow bump
- **WHEN** the dependency upgrade change is complete
- **THEN** `Cargo.toml` still specifies `arrow-array = "57"` and `arrow-schema = "57"`

### Requirement: Upstream tracking documented
The change artifacts SHALL document the upstream dependency chain (`spacebot → lancedb 0.27 → lance 4.0 → arrow 57`) and the specific PR to watch (lance-format/lance#6496).

#### Scenario: Tracking information recorded
- **WHEN** a developer checks the change artifacts
- **THEN** they find the upstream PR reference and estimated timeline (2-6 weeks from 2026-04-15)

### Requirement: Zero code changes expected when unblocked
When arrow 58 becomes available via a new lancedb release, the upgrade SHALL require only version bumps in `Cargo.toml` (lancedb, lance-index, arrow-array, arrow-schema) and no source code changes. Arrow 58 breaking changes do not affect our usage of `RecordBatch`, `StringArray`, `FixedSizeListArray`, `Schema`, `Field`, `DataType`.

#### Scenario: Future upgrade is version-bump only
- **WHEN** a new lancedb version with arrow 58 is published
- **THEN** bumping versions in `Cargo.toml` and running `cargo build` succeeds without source changes in `src/memory/lance.rs` or any other file
