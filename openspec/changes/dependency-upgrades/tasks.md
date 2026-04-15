## 0. Pre-flight

- [x] 0.1 Run `cargo audit`, `bun audit` (interface + docs) and record baseline counts
- [x] 0.2 Run `cargo check --all-targets`, `cargo clippy --all-targets`, `cargo test --lib` to confirm clean state
- [x] 0.3 Verify Rust toolchain >= 1.85 (`rustc --version`)

## 1. Security-Critical + Zero-Risk Upgrades (Phase 1)

- [x] 1.1 Bump prometheus version in Cargo.toml from `"0.13"` to `"0.14"` (fixes RUSTSEC-2024-0437)
- [x] 1.2 Verify prometheus label value API compiles (grep `with_label_values` in `src/telemetry/`)
- [x] 1.3 Bump tokio-tungstenite in Cargo.toml from `"0.28"` to `"0.29"`
- [x] 1.4 Bump dialoguer in Cargo.toml from `"0.11"` to `"0.12"`
- [x] 1.5 Bump cron in Cargo.toml from `"0.12"` to `"0.16"`
- [x] 1.6 Bump notify in Cargo.toml from `"7"` to `"8"` (fixes RUSTSEC-2024-0384)
- [x] 1.7 Check notify feature flags — rename `crossbeam` to `crossbeam-channel` if present
- [x] 1.8 Run `cargo update` to resolve transitive deps
- [x] 1.9 Run `cargo check --all-targets` + `cargo clippy --all-targets`
- [x] 1.10 Run `cargo test --lib` (expect 819 passing)
- [x] 1.11 Run `cargo test --lib -- cron` to verify cron parsing
- [x] 1.12 Run `cargo audit` — verify RUSTSEC-2024-0437 and RUSTSEC-2024-0384 are gone
- [x] 1.13 Commit: `deps: upgrade prometheus (fixes CVE), tokio-tungstenite, dialoguer, cron, notify`

## 2. Minor Code Changes (Phase 2)

- [x] 2.1 Bump fastembed in Cargo.toml from `"4"` to `"5"`
- [x] 2.2 Refactor `src/memory/embedding.rs`: wrap `TextEmbedding` in `Arc<std::sync::Mutex<TextEmbedding>>`
- [x] 2.3 Update all `embed()` call sites to acquire the Mutex lock
- [x] 2.4 Bump chromiumoxide in Cargo.toml from `"0.8"` to `"0.9"`
- [x] 2.5 Bump chromiumoxide_cdp in Cargo.toml from `"0.8"` to `"0.9"`
- [x] 2.6 Verify `src/tools/browser.rs` compiles with new CDP types
- [x] 2.7 Bump rand in Cargo.toml from `"0.9"` to `"0.10"`
- [x] 2.8 In `src/agent/invariant_harness.rs`: `rand::Rng` → `rand::RngExt as _` for `random_range()` method
- [x] 2.9 In `src/secrets/store.rs` and `src/auth.rs`: `rand::RngCore` → `rand::Rng` (RngCore was renamed in 0.10)
- [x] 2.10 StdRng not cloned anywhere; `rand::rng()` returns new type but same API
- [x] 2.11 Run `cargo update` + `cargo check --all-targets` + `cargo clippy --all-targets`
- [x] 2.12 Run `cargo test --lib` (819 passing)
- [x] 2.13 Commit: `deps: upgrade fastembed, chromiumoxide, rand with code changes`

## 3. Frontend Upgrades (Phase 3)

- [ ] 3.1 Run `bunx @andrewbranch/ts5to6` in both `interface/` and `docs/`
- [ ] 3.2 Bump TypeScript to 6.0 in interface: `cd interface && bun add -D typescript@^6.0.2`
- [ ] 3.3 Bump TypeScript to 6.0 in docs: `cd docs && bun add -D typescript@^6.0.2`
- [ ] 3.4 Verify `"types": ["vite/client"]` is still present in `interface/tsconfig.json` after TS6 migration (already exists, but migration tool may modify it)
- [ ] 3.5 Run `cd interface && bunx tsc --noEmit` — fix any new errors
- [ ] 3.6 Run `cd docs && bun run build` — verify build passes
- [ ] 3.7 Grep docs/ for removed lucide brand icons (Chromium, Github, Figma, etc.)
- [ ] 3.8 Bump lucide-react in docs: `cd docs && bun add lucide-react@^1.8.0`
- [ ] 3.9 Run `cd docs && bun run build` — verify build passes
- [ ] 3.10 Commit: `deps(frontend): upgrade TypeScript 6.0, lucide-react 1.8`

## 4. Moderate Refactors (Phase 4 — one PR per task group)

### 4A. LanceDB + Arrow Ecosystem

- [ ] 4A.1 Bump lancedb (`"0.27"`), lance-index (`"4.0"`), arrow-array (`"58"`), arrow-schema (`"58"`) in Cargo.toml
- [ ] 4A.2 Refactor `src/memory/lance.rs`: replace `RecordBatchIterator` with `Vec<RecordBatch>` in `create_empty_table()` and `store()`
- [ ] 4A.3 Run `cargo check --all-targets`
- [ ] 4A.4 Run `cargo test --lib -- memory` + `cargo test --lib -- lance`
- [ ] 4A.5 Verify `cargo audit` — check if lru 0.12.5 warning is resolved
- [ ] 4A.6 Commit: `deps: upgrade lancedb 0.27, arrow 58, lance-index 4.0`

### 4B. Bollard

- [ ] 4B.1 Bump bollard in Cargo.toml from `"0.18"` to `"0.20"`
- [ ] 4B.2 Read `src/update.rs` and map every bollard API call to 0.20 equivalents
- [ ] 4B.3 Replace removed option structs with new query parameter types
- [ ] 4B.4 Check if `IdResponse` is used in `src/update.rs` — if present, rename `.ID` → `.Id`; if not, skip
- [ ] 4B.5 Run `cargo check --all-targets`
- [ ] 4B.6 Commit: `deps: upgrade bollard 0.20 with API migration`

### 4C. Twitch-irc (requires Phase 1 prometheus upgrade)

- [ ] 4C.1 Bump twitch-irc in Cargo.toml from `"5.0"` to `"6.0"`
- [ ] 4C.2 Update `IRCTags` handling: remove `Option` unwrapping from tag values
- [ ] 4C.3 Remove calls to deprecated `ban()`/`unban()`/`timeout()`/`untimeout()` if present
- [ ] 4C.4 Rename `follwers_only` → `followers_only`
- [ ] 4C.5 Run `cargo check --all-targets`
- [ ] 4C.6 Verify Cargo.lock no longer contains `rustls 0.21` or `reqwest 0.11`
- [ ] 4C.7 Commit: `deps: upgrade twitch-irc 6.0, eliminate old rustls/reqwest`

### 4D. Zip

- [ ] 4D.1 Bump zip in Cargo.toml from `"2"` to `"8"`
- [ ] 4D.2 Update DateTime usage in `src/api/system.rs` and `src/skills/installer.rs`
- [ ] 4D.3 Handle `last_modified_time` → `Option<DateTime>` change
- [ ] 4D.4 Remove references to eliminated feature flags
- [ ] 4D.5 Run `cargo check --all-targets`
- [ ] 4D.6 Commit: `deps: upgrade zip 2 → 8 with DateTime API migration`

## 5. Final Verification

- [ ] 5.1 Run full gate: `cargo check`, `cargo clippy`, `cargo test --lib`, `cargo fmt --check`
- [ ] 5.2 Run `cargo audit` and record final vulnerability/warning counts
- [ ] 5.3 Run `cd interface && bunx tsc --noEmit`
- [ ] 5.4 Run `cd docs && bun run build`
- [ ] 5.5 Run `bun audit` in interface/ and docs/
- [ ] 5.6 Compare final audit counts against Phase 0 baseline
- [ ] 5.7 Update `.scratchpad/dependency-upgrade-analysis.md` with final results
