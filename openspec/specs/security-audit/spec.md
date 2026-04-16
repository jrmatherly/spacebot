# Security Audit

## Purpose
Security vulnerability management for the dependency tree. Covers audit tooling, advisory handling, and CI enforcement.
## Requirements
### Requirement: No vulnerable rustls-webpki in dependency tree
The project SHALL NOT include `rustls-webpki` version 0.102.x in its compiled dependency tree.

#### Scenario: Vulnerable webpki removed after serenity upgrade
- GIVEN the serenity Discord library is pinned to the next branch
- WHEN `cargo tree -i rustls-webpki@0.102.8` is run
- THEN the output is "nothing to print" (no matches)

#### Scenario: Safe webpki used by all TLS consumers
- GIVEN the dependency tree is inspected
- WHEN `cargo tree -i rustls-webpki` is run
- THEN only version 0.103.x appears

### Requirement: cargo audit passes with only documented ignores
The CI `cargo audit` command SHALL pass with exit code 0, ignoring only `RUSTSEC-2023-0071` (rsa, no fix available).

#### Scenario: Audit passes clean
- GIVEN `cargo audit --ignore RUSTSEC-2023-0071` is run
- WHEN the audit completes
- THEN the output shows "0 vulnerabilities found"

#### Scenario: CI audit job is a hard gate
- GIVEN the Security Audit CI job runs
- WHEN it executes
- THEN it SHALL NOT have `continue-on-error: true`

### Requirement: Discord adapter functionality preserved
The serenity upgrade SHALL NOT break Discord messaging functionality.

#### Scenario: Compilation succeeds
- GIVEN the serenity upgrade is applied
- WHEN `cargo check` is run
- THEN `src/messaging/discord.rs` compiles without errors

#### Scenario: Unit tests pass
- GIVEN the serenity upgrade is applied
- WHEN `cargo test --lib` is run
- THEN all tests pass (819+)

### Requirement: rsa advisory documented as accepted risk
The `rsa` 0.9.10 advisory (RUSTSEC-2023-0071) SHALL be explicitly ignored in CI with a comment explaining why.

#### Scenario: Ignore is documented
- GIVEN `.github/workflows/ci.yml` is read
- WHEN the `cargo audit` command is inspected
- THEN it includes `--ignore RUSTSEC-2023-0071` with a comment explaining the crate is never compiled

### Requirement: Prometheus upgraded to resolve protobuf CVE
The project SHALL upgrade prometheus from 0.13 to 0.14 to resolve RUSTSEC-2024-0437 (protobuf uncontrolled recursion).

#### Scenario: Protobuf vulnerability resolved
- GIVEN the prometheus upgrade is applied
- WHEN `cargo audit` is run
- THEN RUSTSEC-2024-0437 does not appear in the output

#### Scenario: Prometheus metrics still functional
- GIVEN the project is compiled with the `metrics` feature
- WHEN compilation runs
- THEN all `CounterVec`, `HistogramVec`, `IntCounterVec`, `IntGaugeVec` usages in `src/telemetry/` compile without error

### Requirement: Notify upgraded to resolve instant unmaintained warning
The project SHALL upgrade notify from 7 to 8 to resolve RUSTSEC-2024-0384 (`instant` crate unmaintained).

#### Scenario: Instant warning resolved
- GIVEN the notify upgrade is applied
- WHEN `cargo audit` is run
- THEN RUSTSEC-2024-0384 does not appear in the warnings

#### Scenario: File watcher still functional
- GIVEN config file changes are made while the daemon is running
- WHEN the changes are detected
- THEN the `Watcher` in `src/config/watcher.rs` triggers hot-reload

### Requirement: Deferred upstream-blocked advisories documented in-repo
The repository SHALL include `docs/security/deferred-advisories.md` listing every open Dependabot alert in spacebot-owned code whose resolution is blocked on an upstream crate or package update. Each entry SHALL include: package name, GHSA identifier, severity, current version in lockfile, patched version, blocker (upstream crate/package responsible for the delay), and the trigger that would unblock local resolution.

#### Scenario: Lexical-core entry present
- GIVEN the deferred-advisories doc has been created
- WHEN `docs/security/deferred-advisories.md` is read
- THEN it contains an entry for alert #1 (lexical-core 0.7.6, GHSA-2326-pfpj-vx3h) naming `imap` 3.x migration as the blocker

#### Scenario: Lru entry present
- GIVEN the deferred-advisories doc has been created
- WHEN `docs/security/deferred-advisories.md` is read
- THEN it contains an entry for alert #3 (lru 0.12.5, GHSA-rhfx-m35p-ff5j) naming `lancedb`/`tantivy` as the blocker chain

#### Scenario: Rand root entry present
- GIVEN the deferred-advisories doc has been created
- WHEN `docs/security/deferred-advisories.md` is read
- THEN it contains an entry for alert #15 (rand 0.8.5 in root `Cargo.lock`, GHSA-cq8v-f236-94qc) naming `rig-core` and `lancedb` as blockers

#### Scenario: Rand desktop entry present
- GIVEN the deferred-advisories doc has been created
- WHEN `docs/security/deferred-advisories.md` is read
- THEN it contains an entry for alert #18 (rand 0.8.5 in `desktop/src-tauri/Cargo.lock`, GHSA-cq8v-f236-94qc) naming the tauri transitive chain as the blocker

### Requirement: Deferred-advisories doc referenced from contributor guide
`CONTRIBUTING.md` SHALL include a reference to `docs/security/deferred-advisories.md` under a security-related section so that new contributors discover the doc during normal onboarding.

#### Scenario: Reference exists in contributor guide
- GIVEN `docs/security/deferred-advisories.md` exists
- WHEN `CONTRIBUTING.md` is searched for the string `deferred-advisories.md`
- THEN at least one match is found and the surrounding prose describes the file's purpose

### Requirement: Dependabot update-PR config covers every shipped-code manifest
`.github/dependabot.yml` SHALL contain an `updates:` entry for every directory containing a lockfile or `package.json` that contributes to a shipped spacebot artifact. Vendored upstream directories under `spacedrive/` SHALL NOT have entries.

#### Scenario: Desktop cargo entry present
- GIVEN the dependabot config has been expanded
- WHEN `.github/dependabot.yml` is parsed
- THEN it contains an entry with `package-ecosystem: "cargo"` and `directory: "/desktop/src-tauri"`

#### Scenario: Api-client npm entry present
- GIVEN the dependabot config has been expanded
- WHEN `.github/dependabot.yml` is parsed
- THEN it contains an entry with `package-ecosystem: "npm"` and `directory: "/packages/api-client"`

#### Scenario: Spaceui npm entries present
- GIVEN the dependabot config has been expanded
- WHEN `.github/dependabot.yml` is parsed
- THEN it contains entries with `package-ecosystem: "npm"` for `/spaceui`, `/spaceui/.storybook`, and `/spaceui/examples/showcase`

#### Scenario: Docs npm entry present
- GIVEN the dependabot config has been expanded
- WHEN `.github/dependabot.yml` is parsed
- THEN it contains an entry with `package-ecosystem: "npm"` and `directory: "/docs"`

#### Scenario: Spacedrive NOT in updates
- GIVEN the dependabot config has been expanded
- WHEN `.github/dependabot.yml` is parsed
- THEN no entry has a `directory:` value starting with `/spacedrive`

### Requirement: Dependabot config scope clarified in-file
`.github/dependabot.yml` SHALL include a top-of-file comment stating that this file controls update-PR scoping only, NOT security-alert visibility.

#### Scenario: Scope comment present
- GIVEN the dependabot config has been expanded
- WHEN the first 10 lines of `.github/dependabot.yml` are read
- THEN a YAML comment describes the distinction between update-PR scoping (controlled by this file) and security-alert visibility (controlled separately by per-alert dismissal)

### Requirement: Non-dismissal policy for upstream-blocked advisories
Open Dependabot alerts in spacebot-owned code that are blocked on upstream crate or package updates SHALL NOT be dismissed via the GitHub API. They SHALL remain open and be tracked in `docs/security/deferred-advisories.md`.

#### Scenario: Deferred alerts remain open
- GIVEN the change has been merged
- WHEN the GitHub Dependabot alerts dashboard is queried for each alert listed in `docs/security/deferred-advisories.md`
- THEN each shows `state: open`

### Requirement: Spacedrive vendored-path alerts re-triaged at integration time
Any OpenSpec change that proposes runtime coupling between spacebot and the vendored `spacedrive/` platform (HTTP, IPC, or equivalent) SHALL include an explicit task to re-triage every Dependabot alert whose `manifest_path` starts with `spacedrive/`.

#### Scenario: Integration change includes re-triage task
- GIVEN an OpenSpec change is proposed whose scope includes spacebot calling into a Spacedrive runtime
- WHEN the change's `tasks.md` is reviewed
- THEN it includes at least one task that references `spacedrive/` Dependabot alerts and the decision path (fix upstream / work around / accept risk)

### Requirement: Vite-upgrade alerts verified fixed post-merge
After the vite upgrade lands on `main`, the three Dependabot alerts covered by GHSA-4w7w-66w2-5vf9 in spaceui manifests (alerts #35 and #36 at the time of this change; alert numbers may differ if already closed) SHALL transition to `state: fixed` on the next Dependabot rescan. If they do not, the non-dismissal policy applies: leave them open and investigate the rescan, rather than manually dismissing.

#### Scenario: Post-merge vite alert state verified
- GIVEN the vite upgrade has been merged and Dependabot rescan has completed
- WHEN `gh api repos/jrmatherly/spacebot/dependabot/alerts --paginate` is queried
- THEN alerts #35 and #36 (or their equivalents by GHSA and manifest path) show `state: fixed`, OR the upgrade is documented as blocked in `docs/security/deferred-advisories.md`

### Requirement: Glib-upgrade alert verified fixed post-merge
After the glib upgrade lands on `main`, the Dependabot alert for `glib` in `desktop/src-tauri/Cargo.lock` (alert #17 at the time of this change) SHALL transition to `state: fixed` on the next Dependabot rescan. The non-dismissal policy applies if the rescan does not update the state.

#### Scenario: Post-merge glib alert state verified
- GIVEN the glib upgrade has been merged and Dependabot rescan has completed
- WHEN `gh api repos/jrmatherly/spacebot/dependabot/alerts/17` (or its equivalent if re-numbered) is read
- THEN it shows `state: fixed`, OR the upgrade is documented as blocked in `docs/security/deferred-advisories.md`

