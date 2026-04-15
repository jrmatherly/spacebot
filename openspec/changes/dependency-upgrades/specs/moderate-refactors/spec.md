## ADDED Requirements

### Requirement: LanceDB upgraded to 0.27 with RecordBatch API migration
The project SHALL upgrade lancedb (0.26→0.27), lance-index (2.0→4.0), arrow-array (57→58), and arrow-schema (57→58) together. The `create_table()` and `Table::add()` calls in `src/memory/lance.rs` SHALL be refactored from `RecordBatchIterator` to `Vec<RecordBatch>`.

#### Scenario: Memory store creates tables successfully
- **WHEN** a new embedding table is created via `create_empty_table()`
- **THEN** the table SHALL be created using the new `Vec<RecordBatch>` API

#### Scenario: Vector search returns results
- **WHEN** a semantic search query is executed
- **THEN** results SHALL be returned with the same ranking quality as before the upgrade

### Requirement: Bollard upgraded to 0.20
The project SHALL upgrade bollard from 0.18 to 0.20 in `src/update.rs`. Removed option structs SHALL be replaced with their 0.20 equivalents. `IdResponse.ID` SHALL be updated to `IdResponse.Id`.

#### Scenario: Docker self-update compiles
- **WHEN** bollard is bumped to 0.20 and all API calls are updated
- **THEN** `cargo check --all-targets` SHALL pass

### Requirement: Twitch-irc upgraded to 6.0
The project SHALL upgrade twitch-irc from 5 to 6 in `src/messaging/twitch.rs`. The `IRCTags` type change (`Option<String>` → `String`), removed moderation methods, and `follwers_only` → `followers_only` typo fix SHALL be applied. This upgrade requires prometheus 0.14 first.

#### Scenario: Twitch messaging compiles after upgrade
- **WHEN** twitch-irc is bumped to 6.0 with all code changes applied
- **THEN** `cargo check --all-targets` SHALL pass

#### Scenario: Old TLS stack eliminated
- **WHEN** twitch-irc 6.0 is installed
- **THEN** `rustls 0.21.12` and `reqwest 0.11.27` SHALL no longer appear in Cargo.lock

### Requirement: Zip upgraded to 8.x
The project SHALL upgrade zip from 2.4 to 8.x in `src/api/system.rs` and `src/skills/installer.rs`. DateTime API changes and feature flag removals SHALL be addressed.

#### Scenario: Skill installation works
- **WHEN** a skill archive is imported via the installer
- **THEN** `ZipArchive` SHALL extract files successfully

#### Scenario: Config export works
- **WHEN** a config export is triggered via the system API
- **THEN** `ZipWriter` SHALL produce a valid archive
