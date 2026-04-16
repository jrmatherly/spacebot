# Rust Dependencies

## Purpose
Version currency and API compatibility for Rust crate dependencies in Cargo.toml. Covers upgrade requirements, migration constraints, and compilation guarantees.

## Requirements

### Requirement: Rig-core at 0.35
The system SHALL use `rig-core` version 0.35 in `Cargo.toml`. The upgrade SHALL maintain all existing behavior: agent prompting, history management, tool execution, hook-driven nudging, and error recovery.

#### Scenario: Cargo build succeeds
- GIVEN `rig-core` is bumped from `0.33` to `0.35` in `Cargo.toml` and all migration edits are applied
- WHEN `cargo build` is run
- THEN it succeeds with zero errors

#### Scenario: All unit tests pass
- GIVEN the upgrade is complete
- WHEN `cargo test --lib` is run
- THEN all existing tests pass

### Requirement: History reconstruction via extended_details
The system SHALL use `.extended_details()` on `PromptRequest` to obtain updated history from the rig agent loop. The `PromptResponse.messages` field SHALL be merged into the caller's history vec after each successful prompt call.

#### Scenario: prompt_once updates history
- GIVEN `prompt_once` is called and the agent responds successfully
- WHEN the response is processed
- THEN the caller's `history` vec contains all new messages (user prompt, assistant response, any tool calls/results) appended after the call

#### Scenario: prompt_with_tool_nudge_retry updates history across retries
- GIVEN `prompt_with_tool_nudge_retry` runs through one or more nudge retry iterations
- WHEN the final response is processed
- THEN the caller's `history` vec reflects the final state including injected context messages, pruned nudge attempts, and the successful response

#### Scenario: prompt_once_streaming is unaffected
- GIVEN `prompt_once_streaming` is called
- WHEN it runs
- THEN its internal history management (local `chat_history` vec, manual message pushing, write-back) continues to work unchanged

### Requirement: PromptError field construction matches rig 0.35
The system SHALL construct `PromptError::PromptCancelled` and `PromptError::MaxTurnsError` variants with field types matching rig 0.35's definitions. If `chat_history` fields are unboxed in 0.35, all `Box::new()` wrappers SHALL be removed.

#### Scenario: PromptError construction compiles
- GIVEN all `PromptError` construction sites are updated
- WHEN `cargo build` is run
- THEN it succeeds with no type mismatch errors on `chat_history` or `prompt` fields

### Requirement: with_history call sites use immutable references
The system SHALL pass history to `.with_history()` as an immutable reference (`&*history` or `&history`), not a mutable reference. No call site SHALL use `&mut` with `with_history`.

#### Scenario: No mutable history references remain
- GIVEN the upgrade is complete
- WHEN `grep -rn "with_history.*&mut" src/` is run
- THEN zero matches are found

### Requirement: ToolServer changes are transparent
The system SHALL NOT pattern-match on `ToolServerError` variants. The `tool_server_handle` public field on `Agent` SHALL continue to be accessed directly for `.call_tool()` in `prompt_once_streaming`.

#### Scenario: No ToolServerError variant matching
- GIVEN the upgrade is complete
- WHEN `grep -rn "ToolServerError::" src/` is run
- THEN zero matches are found

### Requirement: CompletionModel trait impl unchanged
The `SpacebotModel` implementation of `CompletionModel` in `src/llm/model.rs` SHALL NOT be modified. The trait signatures are unchanged between 0.33 and 0.35.

#### Scenario: model.rs untouched
- GIVEN the upgrade is complete
- WHEN `src/llm/model.rs` is diffed against the pre-upgrade state
- THEN there is no diff

### Requirement: Zero-risk Cargo.toml bumps applied
The project SHALL upgrade tokio-tungstenite (0.28 to 0.29), dialoguer (0.11 to 0.12), and cron (0.12 to 0.16) with zero source code changes required.

#### Scenario: All zero-risk upgrades compile
- GIVEN the version constraints are bumped and `cargo update` is run
- WHEN `cargo check --all-targets` is run
- THEN it passes with no errors

#### Scenario: Cron parsing unchanged
- GIVEN existing cron expressions are parsed via `Schedule::from_str`
- WHEN the same expressions are parsed after the upgrade
- THEN they produce the same next-execution times

### Requirement: Fastembed upgraded with Mutex wrapper
The project SHALL upgrade fastembed from 4 to 5. The `TextEmbedding` model in `src/memory/embedding.rs` SHALL be wrapped in `Arc<std::sync::Mutex<TextEmbedding>>` to satisfy the new `&mut self` requirement on `embed()`.

#### Scenario: Embedding generation works after upgrade
- GIVEN text is submitted for embedding via the memory system
- WHEN embeddings are generated
- THEN they have the same dimensionality (384-dim all-MiniLM-L6-v2)

### Requirement: Chromiumoxide upgraded to 0.9
The project SHALL upgrade chromiumoxide and chromiumoxide_cdp from 0.8 to 0.9 in lockstep. The browser tool in `src/tools/browser.rs` SHALL compile against the new CDP protocol version.

#### Scenario: Browser tool compiles
- GIVEN both chromiumoxide crates are bumped to 0.9
- WHEN `cargo check --all-targets` is run
- THEN it passes

### Requirement: Rand upgraded to 0.10
The project SHALL upgrade rand from 0.9 to 0.10. In `src/agent/invariant_harness.rs`, `rand::Rng` extension methods SHALL use the new `rand::RngExt` trait if needed. In `src/secrets/store.rs` and `src/auth.rs`, `RngCore` (used for `fill_bytes()`) is unchanged and SHALL require no modifications.

#### Scenario: Cryptographic operations functional
- GIVEN secret key generation or auth token generation is invoked
- WHEN random bytes are generated
- THEN `RngCore::fill_bytes()` works correctly

#### Scenario: Test harness RNG functional
- GIVEN the invariant harness creates a seeded RNG
- WHEN `StdRng::seed_from_u64()` and range methods are used
- THEN they work with the updated trait imports

### Requirement: LanceDB upgraded to 0.27 with RecordBatch API migration
The project SHALL upgrade lancedb (0.26 to 0.27), lance-index (2.0 to 4.0), arrow-array (57 to 58), and arrow-schema (57 to 58) together. The `create_table()` and `Table::add()` calls in `src/memory/lance.rs` SHALL be refactored from `RecordBatchIterator` to `Vec<RecordBatch>`.

#### Scenario: Memory store creates tables successfully
- GIVEN a new embedding table is created via `create_empty_table()`
- WHEN the table creation runs
- THEN the table is created using the new `Vec<RecordBatch>` API

#### Scenario: Vector search returns results
- GIVEN a semantic search query is executed
- WHEN results are returned
- THEN they have the same ranking quality as before the upgrade

### Requirement: Bollard upgraded to 0.20
The project SHALL upgrade bollard from 0.18 to 0.20 in `src/update.rs`. Removed option structs SHALL be replaced with their 0.20 equivalents. `IdResponse.ID` SHALL be updated to `IdResponse.Id`.

#### Scenario: Docker self-update compiles
- GIVEN bollard is bumped to 0.20 and all API calls are updated
- WHEN `cargo check --all-targets` is run
- THEN it passes

### Requirement: Twitch-irc upgraded to 6.0
The project SHALL upgrade twitch-irc from 5 to 6 in `src/messaging/twitch.rs`. The `IRCTags` type change (`Option<String>` to `String`), removed moderation methods, and `follwers_only` to `followers_only` typo fix SHALL be applied. This upgrade requires prometheus 0.14 first.

#### Scenario: Twitch messaging compiles after upgrade
- GIVEN twitch-irc is bumped to 6.0 with all code changes applied
- WHEN `cargo check --all-targets` is run
- THEN it passes

#### Scenario: Old TLS stack eliminated
- GIVEN twitch-irc 6.0 is installed
- WHEN `cargo tree` is inspected
- THEN `rustls 0.21.12` and `reqwest 0.11.27` no longer appear in Cargo.lock

### Requirement: Zip upgraded to 8.x
The project SHALL upgrade zip from 2.4 to 8.x in `src/api/system.rs` and `src/skills/installer.rs`. DateTime API changes and feature flag removals SHALL be addressed.

#### Scenario: Skill installation works
- GIVEN a skill archive is imported via the installer
- WHEN `ZipArchive` extraction runs
- THEN files are extracted successfully

#### Scenario: Config export works
- GIVEN a config export is triggered via the system API
- WHEN `ZipWriter` produces the archive
- THEN the archive is valid

### Requirement: Arrow upgrade blocked upstream
The system SHALL remain on `arrow-array` 57 and `arrow-schema` 57 until lancedb publishes a version supporting arrow 58 to crates.io. No manual version override or feature patching SHALL be attempted.

#### Scenario: No premature arrow bump
- GIVEN the dependency upgrade is complete
- WHEN `Cargo.toml` is inspected
- THEN it specifies `arrow-array = "57"` and `arrow-schema = "57"`

### Requirement: Upstream tracking documented
The change artifacts SHALL document the upstream dependency chain (`spacebot` to `lancedb 0.27` to `lance 4.0` to `arrow 57`) and the specific PR to watch (lance-format/lance#6496).

#### Scenario: Tracking information recorded
- GIVEN a developer checks the change artifacts
- WHEN they look for the upstream PR reference
- THEN they find it with an estimated timeline (2-6 weeks from 2026-04-15)

### Requirement: Zero code changes expected when unblocked
When arrow 58 becomes available via a new lancedb release, the upgrade SHALL require only version bumps in `Cargo.toml` and no source code changes.

#### Scenario: Future upgrade is version-bump only
- GIVEN a new lancedb version with arrow 58 is published
- WHEN versions are bumped in `Cargo.toml` and `cargo build` is run
- THEN it succeeds without source changes

### Requirement: Full gate passes
The system SHALL pass all delivery gates after dependency upgrades.

#### Scenario: gate-pr succeeds
- GIVEN the upgrade is complete
- WHEN `just gate-pr` is run
- THEN it passes (formatting, compile, clippy, migration safety, tests)
