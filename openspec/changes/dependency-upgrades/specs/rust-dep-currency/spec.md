## ADDED Requirements

### Requirement: Zero-risk Cargo.toml bumps applied
The project SHALL upgrade tokio-tungstenite (0.28→0.29), dialoguer (0.11→0.12), and cron (0.12→0.16) with zero source code changes required.

#### Scenario: All zero-risk upgrades compile
- **WHEN** the version constraints are bumped and `cargo update` is run
- **THEN** `cargo check --all-targets` SHALL pass with no errors

#### Scenario: Cron parsing unchanged
- **WHEN** existing cron expressions are parsed via `Schedule::from_str`
- **THEN** the same expressions SHALL parse successfully and produce the same next-execution times

### Requirement: Fastembed upgraded with Mutex wrapper
The project SHALL upgrade fastembed from 4 to 5. The `TextEmbedding` model in `src/memory/embedding.rs` SHALL be wrapped in `Arc<std::sync::Mutex<TextEmbedding>>` to satisfy the new `&mut self` requirement on `embed()`.

#### Scenario: Embedding generation works after upgrade
- **WHEN** text is submitted for embedding via the memory system
- **THEN** embeddings SHALL be generated with the same dimensionality (384-dim all-MiniLM-L6-v2)

### Requirement: Chromiumoxide upgraded to 0.9
The project SHALL upgrade chromiumoxide and chromiumoxide_cdp from 0.8 to 0.9 in lockstep. The browser tool in `src/tools/browser.rs` SHALL compile against the new CDP protocol version.

#### Scenario: Browser tool compiles
- **WHEN** both chromiumoxide crates are bumped to 0.9
- **THEN** `cargo check --all-targets` SHALL pass

### Requirement: Rand upgraded to 0.10
The project SHALL upgrade rand from 0.9 to 0.10. In `src/agent/invariant_harness.rs`, `rand::Rng` extension methods (e.g., `random_range()`) SHALL use the new `rand::RngExt` trait if needed. In `src/secrets/store.rs` and `src/auth.rs`, `RngCore` (used for `fill_bytes()`) is unchanged and SHALL require no modifications.

#### Scenario: Cryptographic operations functional
- **WHEN** secret key generation or auth token generation is invoked
- **THEN** random bytes SHALL be generated successfully via `RngCore::fill_bytes()`

#### Scenario: Test harness RNG functional
- **WHEN** the invariant harness creates a seeded RNG
- **THEN** `StdRng::seed_from_u64()` and any range methods SHALL work with the updated trait imports
